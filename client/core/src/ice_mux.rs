// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # ice_mux — ONE protected UDP socket for ICE *and* WireGuard
//! (official-alignment audit §2.7 **stages 2-3**; upstream `UniversalUDPMuxDefault`)
//!
//! Upstream NetBird does not give an ICE agent a socket: it gives it a *mux*.
//! ONE UDP socket for the whole client serves every peer's connectivity checks
//! AND the tunnel data — because upstream's mux rides the WireGuard device's own
//! socket. This module is that socket:
//!
//! | behaviour | upstream (file:line) |
//! | --- | --- |
//! | one socket for all agents, and it is the WG socket | `client/iface/bind/ice_bind.go:291-316` (`createOrUpdateMux` over the WG bind's conn), `client/iface/device/device_kernel_unix.go:97-113` (mux over `sharedsock.Listen(wgPort)`) |
//! | the mux RUNS the reader (a real thread) | `device_kernel_unix.go:109` (`go mux.ReadFromConn(t.ctx)`), `universal.go:80-108` |
//! | non-STUN goes straight to WireGuard | `ice_bind.go:209-287` (the WG receive fn keeps the buffer the STUN filter did not consume), `:403-421` (`isWireGuardMsg`) |
//! | read loop sized MTU+overhead | `universal.go:81`, `client/iface/bufsize` |
//! | srflx (XOR-MAPPED-ADDRESS) cache per STUN server | `universal.go:28-31`, `:218-270` |
//! | cache TTL 25 s, set at REQUEST time | `universal.go:38`, `:49-51`, `:285` |
//! | in-flight de-duplication (waiter channel) | `universal.go:276-301`, `:321-323` |
//! | STUN from an overlay source is dropped | `universal.go:160-173` |
//! | Binding Request → session by USERNAME ufrag | `udpmux/mux.go:455-468` (`findConnectionByUsername`) |
//! | other packets → session by remote address | `udpmux/mux.go:425-452` (`forwardToAllConnections` + `addressMap`) |
//! | address registry filled by every write | `udpmux/conn.go:81-92` (`WriteTo` → `addAddress`) |
//! | session teardown unregisters, never closes the shared fd | `udpmux/mux.go:284-321` (`RemoveConnByUfrag`), `udpmux/conn.go:114-121` |
//! | the mux never closes the socket it was built over | `ice_bind.go:308-309` ("Don't close the old mux - it doesn't own the underlying connections") |
//! | host candidates = listen address × mux port | fork `gather.go:331-405` (`gatherCandidatesLocalUDPMux`) |
//! | srflx candidate `raddr/rport` = the mux listen address | fork `gather.go:491-513` (`gatherCandidatesSrflxUDPMux`) |
//!
//! ## The reader thread and its back-pressure
//!
//! [`IceMux::start_reader`] is upstream's `go mux.ReadFromConn` (stage 3);
//! until it is called the mux is driven by whoever calls [`IceMux::drain`]
//! (the ICE pump, or an injected-clock host harness). The loop is:
//! `poll(POLLIN, MUX_READER_POLL_MS)` → one bounded [`MUX_DRAIN_BUDGET`] pass →
//! repeat. Back-pressure is structural, not buffered: a pass reads a bounded
//! number of datagrams and every non-STUN datagram is handed to
//! [`MuxUdpSink::handle_udp`] **synchronously**, so a slow WG/TUN consumer
//! slows the read loop itself and the kernel socket buffer then throttles the
//! peer — the same shape as WG's own receive loop upstream. The STUN half of
//! the demux stays queued per session, bounded, dropping the newest with a
//! counter (the retired non-STUN per-session data queue is gone — stage 4).
//!
//! ## Where the socket comes from
//!
//! The mux does NOT own the socket's lifetime in the upstream sense either: it
//! is built over an ALREADY BOUND, ALREADY PROTECTED fd (the shell-protected WG
//! outer socket, `wg_fwd_open` → `VpnConnection.protect` → feed), dups it, and
//! closes **only its own dup**. Sessions close nothing (their candidates are map
//! entries, [`crate::ice_session::IceSession::stop`]) and the WG device closes
//! only its own dup. There is exactly ONE `open` + `bind` + `protect` for ICE
//! and WG together, and there is NO unprotected fallback: no mux ⇒ no local
//! candidates ⇒ the peer stays Idle with an explicit `ice:no-mux`.
//!
//! ## What this module deliberately is NOT
//!
//! * **No TURN / relay address.** Upstream's `GetRelayedAddr` is not implemented
//!   there either (`universal.go:146-150`): relay candidates come from the relay
//!   carrier, not from this socket, and the relayed WG traffic never touches it.
//!
//! ## Ownership (governance §二.4, fail-closed)
//!
//! The fd is PROTECTED before it ever carries a datagram, and there is exactly
//! ONE such fd for both planes: the core opens and binds the WG outer socket
//! wildcard (`wg_fwd_open`), the shell `VpnConnection.protect`s that number and
//! feeds it back (`connector_wg_socket_feed`), and the mux is then built OVER
//! that same protected socket (`connector_ice_mux_arm` →
//! [`crate::peer_conn::PeerIceOrchestrator::arm_mux_from_wg_socket`] →
//! [`IceMux::adopt`]) — upstream's `createOrUpdateMux` shape
//! (`client/iface/bind/ice_bind.go:291-316`). [`IceMux::open_wildcard`] is the
//! host path (the host CLI has no shell to protect anything) and the test path.
//! There is NO unprotected fallback: no mux ⇒ no local candidates ⇒ the peer
//! stays Idle with an explicit `ice:no-mux` network error.

use std::collections::{HashMap, VecDeque};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::ice::{seam_to_management, set_nonblock};
use crate::ice_session::demux_routes_to_wg;
use crate::management::ManagementError;
use crate::peer_conn::{PumpWake, PumpWaker};
use crate::stun::{self, TransactionId, BINDING_REQUEST};
use crate::sys;

/// Upstream `XORMappedAddrCacheTTL` default: `time.Second * 25`
/// (`client/iface/udpmux/universal.go:49-51`). It is armed when the STUN
/// request is SENT and never extended by the response
/// (`universal.go:284-288`, `:198-216` — `handleXORMappedResponse` only sets
/// the address), so this is the age after which the next agent re-queries.
pub const XOR_MAPPED_CACHE_TTL_MS: u64 = 25_000;

/// Read buffer: upstream `MTU + bufsize.WGBufferOverhead`
/// (`universal.go:81`, `client/iface/bufsize`). Ours is the WG device's own
/// `WG_BUF = 2048` (`wg_device.rs`), which already covers the default MTU
/// 1280 (upstream `iface.DefaultMTU`) + overhead; a short buffer would
/// silently truncate WireGuard datagrams the moment stage 3 routes them
/// through here.
pub const MUX_RECV_BUF: usize = 2048;

/// Datagrams one [`IceMux::drain`] call may read. The **reader thread** calls
/// it once per readable pass ([`MUX_READER_POLL_MS`]) and the ICE pump calls it
/// only when no reader runs (the host/harness shape, where its short
/// `ICE_PUMP_TICK_MS` cadence still applies), so this is a per-pass bound, not
/// a throughput cap: what is left stays in the kernel socket buffer and the
/// level-triggered `poll` reports the fd readable again for the next pass (that
/// is the back-pressure described in the module docs).
const MUX_DRAIN_BUDGET: usize = 256;

/// The reader thread's idle `poll` timeout: how long it may sit in `poll`
/// before it re-checks the stop flag. Bounds the shutdown latency of
/// [`IceMux::stop_reader`]; a ready datagram wakes it immediately.
pub const MUX_READER_POLL_MS: u64 = 50;

/// Per-session inbound STUN queue cap. The session's own
/// `RECV_BUDGET_PER_RUN` (64) drains this every tick, so the cap only bounds a
/// stalled consumer (a session torn down between the drain and its pump).
const MUX_STUN_QCAP: usize = 64;

/// One IPv4 CIDR of the client's OWN overlay network — upstream's
/// `wgaddr.Address.Network`/`IPv6Net` (`universal.go:169-170`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayNet {
    pub addr: [u8; 4],
    pub prefix: u8,
}

impl OverlayNet {
    pub fn contains(&self, ip: [u8; 4]) -> bool {
        let prefix = self.prefix.min(32) as u32;
        if prefix == 0 {
            return true;
        }
        let mask: u32 = u32::MAX << (32 - prefix);
        let a = u32::from_be_bytes(self.addr) & mask;
        let b = u32::from_be_bytes(ip) & mask;
        a == b
    }
}

/// `(address, port)` in our UDP4-only world — upstream's `*net.UDPAddr`
/// (`unmap()`ed there, `:128`, `:168`).
pub type MappedAddr = ([u8; 4], u16);

/// One STUN datagram delivered to a session: the raw bytes and the source it
/// arrived from (upstream's `udpMuxedConn::ReadFrom` `(n, rAddr)` pair).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxStun {
    pub src: MappedAddr,
    pub bytes: Vec<u8>,
}

/// Mux counters (observability; never a protocol decision).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MuxStats {
    pub reads: u64,
    /// srflx cache: served from a live entry.
    pub xor_hits: u64,
    /// srflx cache: no live entry, so a request was sent (or joined in flight).
    pub xor_misses: u64,
    /// srflx cache: entries dropped after the TTL.
    pub xor_expired: u64,
    /// srflx cache: responses that filled an entry.
    pub xor_filled: u64,
    /// STUN datagrams dropped because their source is inside our own overlay.
    pub overlay_dropped: u64,
    /// STUN datagrams that matched no session.
    pub stun_unknown: u64,
    /// Non-STUN datagrams that matched no session.
    pub data_unknown: u64,
    /// Datagrams dropped because a per-session queue was full.
    pub queue_dropped: u64,
}

/// The WireGuard data-plane consumer of the non-STUN datagrams this socket
/// demuxes. Upstream does not have a separate interface for this: the mux
/// either hands the buffer back to WG's own receive function (`ice_bind.go:257-285`)
/// or WG's device reads the socket itself; ours is a `boringtun` device with no
/// `Bind` API, so the mux calls [`WgDevice::handle_udp`] instead —
/// [`crate::wg_device::WgPeerApplier::handle_udp_inbound`] at the seam.
///
/// [`WgDevice::handle_udp`]: crate::wg_device::WgDevice
pub trait MuxUdpSink: Send + Sync + 'static {
    /// Deliver ONE non-STUN datagram, in arrival order, with the source it
    /// arrived from (`(n, rAddr)` of the reader). Synchronous BY DESIGN: this
    /// is where the reader's back-pressure comes from.
    fn handle_udp(&self, datagram: &[u8], src: MappedAddr);
}

/// The shared state the reader thread and every caller hold. Separate from
/// [`IceMux`] so the reader owns an `Arc` that keeps the socket alive exactly
/// until the reader has stopped.
struct Shared {
    st: Mutex<Inner>,
    /// Set by [`IceMux::stop_reader`], checked once per reader pass.
    reader_stop: AtomicBool,
    /// Non-STUN delivery route: the WG device (upstream's WG receive path).
    /// `None` = none armed (a mux built by a test that never attaches a sink):
    /// there is NO per-session fallback queue any more (official-alignment
    /// stage 4), so such a datagram is dropped and counted in `data_unknown`.
    sink: Mutex<Option<Arc<dyn MuxUdpSink>>>,
    /// 0.0.28 event-driven ICE pump: the orchestrator's waker, installed by
    /// [`IceMux::set_waker`]. A drain pass that queued at least one STUN
    /// datagram for a session wakes the pump ONCE (not per datagram) — before
    /// this, the 10 ms pump tick was what made a session read its queue, so a
    /// delivery without a wake would wait for the next coarse deadline.
    /// Pure leaf lock: taken alone, never held while taking `st`.
    waker: Mutex<Option<Arc<PumpWaker>>>,
}

/// What [`Inner::dispatch`] decided about one datagram, so that the sink call
/// happens OUTSIDE the state lock (the sink takes the WG device lock; nothing
/// may take the two in the opposite order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchOut {
    /// Fully handled (cached, or dropped + counted).
    Handled,
    /// Queued to a session's inbound STUN queue (the session half owns it): the
    /// caller wakes the ICE pump once per drain pass.
    HandledStun,
    /// Non-STUN with a sink armed: the caller delivers the SAME buffer through
    /// [`MuxUdpSink::handle_udp`] after dropping the state lock.
    ToSink,
}

/// ONE shared UDP socket: the mux upstream builds over the WireGuard device's
/// own socket (`createOrUpdateMux`, `ice_bind.go:291-316`).
pub struct IceMux {
    /// The owned socket's number, copied at construction: the hot read path
    /// must not take the state lock merely to learn the fd. Valid for as long
    /// as `shared` (which owns the descriptor) is alive.
    fd: sys::c_int,
    shared: Arc<Shared>,
    /// The ONE reader thread, while it runs (`None` = the caller drives
    /// [`IceMux::drain`]).
    reader: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl std::fmt::Debug for IceMux {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.lock();
        f.debug_struct("IceMux")
            .field("fd", &inner.fd.as_raw_fd())
            .field("port", &inner.port)
            .field("sessions", &inner.sessions.len())
            .finish()
    }
}

struct XorEntry {
    /// The mapped address, once a response arrived (upstream `xorMapped.addr`).
    addr: Option<MappedAddr>,
    /// Armed at REQUEST time (`universal.go:285`), never extended.
    expires_at: u64,
    /// The transaction id we sent — stricter than upstream, which matches a
    /// response to a server purely by source address
    /// (`universal.go:187-194`). A server echoes the id, so this only rejects
    /// stale answers from an earlier round on the same server.
    txn: Option<TransactionId>,
    /// In-flight de-duplication: everyone waiting for this server parks its own
    /// channel here (`universal.go:280-289`, `:321-323`).
    waiters: Vec<Sender<Option<MappedAddr>>>,
}

struct Session {
    ufrag: String,
    /// N2 v4.1 P7: the label the outer-source receipt names as `peer=`. The
    /// session owns this fact; the mux reads it only while armed.
    peer_label: String,
    stun: VecDeque<MuxStun>,
}

struct Inner {
    fd: OwnedFd,
    port: u16,
    /// ufrag → session (upstream `connsIPv4`).
    by_ufrag: HashMap<String, u64>,
    /// remote address → session (upstream `addressMap`, filled by `WriteTo`).
    by_addr: HashMap<MappedAddr, u64>,
    sessions: HashMap<u64, Session>,
    /// srflx cache keyed by the STUN server address (upstream `xorMappedMap`).
    xor: HashMap<MappedAddr, XorEntry>,
    overlay: Vec<OverlayNet>,
    stats: MuxStats,
}

impl IceMux {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.shared.st.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Open + bind a wildcard mux socket on `port` (`0` = ephemeral) — the
    /// HOST/TEST path (the host CLI has no shell and no WG device socket to ride).
    pub fn open_wildcard(port: u16) -> Result<Arc<IceMux>, ManagementError> {
        let (fd, bound) = open_bound_wildcard(port)?;
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        IceMux::adopt(owned, vec![], bound)
    }

    /// HOST/TEST path, provider flavour ([`crate::peer_conn::
    /// PeerIceOrchestrator::arm_mux`]): the socket comes from
    /// [`crate::ice::UdpSocketSource`] UNBOUND, so binding it to the wildcard
    /// address on `port` is what happens here. A bind failure is a hard
    /// error ([`ManagementError::Network`]): no mux, no local candidates.
    pub fn adopt_unbound(
        owned: OwnedFd,
        port: u16,
        overlay: Vec<OverlayNet>,
    ) -> Result<Arc<IceMux>, ManagementError> {
        let fd = owned.as_raw_fd();
        let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], port);
        if unsafe { sys::bind(fd, &bind_sa, core::mem::size_of::<sys::sockaddr_in>() as u32) } == -1
        {
            let e = sys::errno();
            drop(owned); // the single close on the failure path
            return Err(ManagementError::Network(format!("ice-mux:bind-failed (errno={e})")));
        }
        IceMux::adopt(owned, overlay, port)
    }

    /// Adopt an ALREADY BOUND (and, on device, already protected) socket as the
    /// mux — the stage-3 path: a dup of the WG device's own outer socket
    /// (upstream `createOrUpdateMux`, `ice_bind.go:291-316`; "the sockets are
    /// managed by WireGuard's StdNetBind, not by us", `:308-309`). `want_port` is
    /// the port the caller asked for: 0 = whichever the kernel picked, otherwise
    /// it must match `getsockname` (a mismatch is a hard error, never a silent
    /// fallback — the same rule the retired gather hand-over applied).
    pub fn adopt(
        owned: OwnedFd,
        overlay: Vec<OverlayNet>,
        want_port: u16,
    ) -> Result<Arc<IceMux>, ManagementError> {
        let fd = owned.as_raw_fd();
        let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        if unsafe { sys::getsockname(fd, &mut name, &mut len) } != 0 {
            return Err(ManagementError::Network("ice-mux:getsockname-failed".into()));
        }
        let port = u16::from_be(name.sin_port);
        if port == 0 {
            return Err(ManagementError::Network("ice-mux:socket-unbound".into()));
        }
        if want_port != 0 && want_port != port {
            return Err(ManagementError::Network("ice-mux:port-mismatch".into()));
        }
        set_nonblock(fd).map_err(seam_to_management)?;
        Ok(Arc::new(IceMux {
            fd,
            shared: Arc::new(Shared {
                st: Mutex::new(Inner {
                    fd: owned,
                    port,
                    by_ufrag: HashMap::new(),
                    by_addr: HashMap::new(),
                    sessions: HashMap::new(),
                    xor: HashMap::new(),
                    overlay,
                    stats: MuxStats::default(),
                }),
                reader_stop: AtomicBool::new(false),
                sink: Mutex::new(None),
                waker: Mutex::new(None),
            }),
            reader: Mutex::new(None),
        }))
    }

    /// The one port every host candidate and every srflx `rport` carries
    /// (upstream: `GetListenAddresses`, `universal.go:141-144`).
    pub fn local_port(&self) -> u16 {
        self.lock().port
    }
    /// BORROWED fd number of the shared socket — the WG device dups it as a
    /// peer egress exactly as it used to dup the selected session socket.
    pub fn raw_fd(&self) -> sys::c_int {
        self.fd
    }

    /// Register a session's local ufrag (`GetConn(a.localUfrag, …)`,
    /// `fork gather.go:388-400`). `id` is opaque to the mux; `peer_label` only
    /// labels outer-source receipt lines.
    pub fn register_session(&self, id: u64, ufrag: &str, peer_label: &str) {
        let mut inner = self.lock();
        inner.by_ufrag.insert(ufrag.to_string(), id);
        inner.sessions.insert(
            id,
            Session {
                ufrag: ufrag.to_string(),
                peer_label: peer_label.to_string(),
                stun: VecDeque::new(),
            },
        );
    }

    /// Unregister a session (`RemoveConnByUfrag`, `udpmux/mux.go:284-321`) — the
    /// SHARED fd is never touched: that is the whole point of the mux.
    pub fn unregister_session(&self, id: u64) {
        let mut inner = self.lock();
        if let Some(s) = inner.sessions.remove(&id) {
            inner.by_ufrag.remove(&s.ufrag);
        }
        inner.by_addr.retain(|_, v| *v != id);
    }

    /// How many sessions are registered (diagnostics/tests).
    pub fn session_count(&self) -> usize {
        self.lock().sessions.len()
    }

    /// `true` while `id` is registered with this mux.
    pub fn is_registered(&self, id: u64) -> bool {
        self.lock().sessions.contains_key(&id)
    }

    /// Send one datagram to `dst` on the shared socket, remembering the address
    /// so that replies route back to this session (upstream `udpMuxedConn::
    /// WriteTo` → `addAddress` → `registerConnForAddress`, `udpmux/conn.go:81-92`,
    /// `udpmux/mux.go:361-376`). Returns `false` when `sendto` refused the
    /// datagram (the caller's check state machine records the failure).
    pub fn send_to(&self, id: u64, dst: MappedAddr, bytes: &[u8]) -> bool {
        let mut inner = self.lock();
        if !inner.sessions.contains_key(&id) {
            return false;
        }
        inner.by_addr.insert(dst, id);
        let fd = inner.fd.as_raw_fd();
        sendto(fd, dst, bytes)
    }

    /// Drain up to `max` inbound STUN datagrams for `id` (upstream: the
    /// session's `ReadFrom` on its `udpMuxedConn`).
    pub fn take_stun(&self, id: u64, max: usize) -> Vec<MuxStun> {
        let mut inner = self.lock();
        let Some(s) = inner.sessions.get_mut(&id) else {
            return Vec::new();
        };
        let n = max.min(s.stun.len());
        s.stun.drain(..n).collect()
    }

    pub fn stats(&self) -> MuxStats {
        self.lock().stats
    }

    /// 0.0.26 gap 1: the NON-BLOCKING counter read for the status document
    /// ([`crate::connector::ConnectorStatus`]'s `ice_mux` tail). The state lock
    /// is held by the reader pass / the ICE pump while it classifies a datagram,
    /// so a busy lock answers `None` (this document's "unknown") instead of
    /// queueing the shell's synchronous status read behind it — the same rule
    /// [`crate::connector::ConnectorShared::ice_status_sample`] applies to the
    /// orchestrator lock. Never a fabricated zero.
    pub fn stats_try(&self) -> Option<MuxStats> {
        self.shared.st.try_lock().ok().map(|g| g.stats)
    }

    /// The read loop, one pass: read until EAGAIN or the per-pass budget, then
    /// classify and route (upstream `UniversalUDPMuxDefault::ReadFromConn`,
    /// `universal.go:80-108`). Non-blocking; whoever calls it owns the cadence —
    /// the reader thread ([`IceMux::start_reader`]) or the ICE pump.
    pub fn drain(&self) -> usize {
        drain_shared(&self.shared, self.fd)
    }

    /// Install the ICE orchestrator's event-driven waker (0.0.28). Idempotent:
    /// re-installing the SAME `Arc` is a no-op (the orchestrator calls it once
    /// per pass so a mux that appears later gets wired too). A drain pass that
    /// queued STUN for a session then calls [`PumpWaker::notify`] once — the
    /// session's queue is consumed by the ICE pump, never by this thread.
    pub fn set_waker(&self, waker: Arc<PumpWaker>) {
        let mut slot = self.shared.waker.lock().unwrap_or_else(|p| p.into_inner());
        if slot.as_ref().is_some_and(|cur| Arc::ptr_eq(cur, &waker)) {
            return;
        }
        *slot = Some(waker);
    }

    /// Attach the WG data-plane consumer of non-STUN datagrams (upstream: the
    /// buffer the STUN filter did not consume goes straight on into WG,
    /// `ice_bind.go:257-285`). Returned `true` when one was already attached.
    /// Idempotent upsert; there is no detach because production never has a
    /// reason to stop delivering WG data while the socket lives.
    pub fn attach_udp_sink(&self, sink: Arc<dyn MuxUdpSink>) -> bool {
        self.shared.sink.lock().unwrap_or_else(|p| p.into_inner()).replace(sink).is_some()
    }

    /// Whether non-STUN datagrams can go straight to WG (a sink is armed) or
    /// would be dropped and counted in `data_unknown` (a test mux that never
    /// attached one — there is no per-session fallback queue any more).
    pub fn sink_armed(&self) -> bool {
        self.shared.sink.lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }

    /// Start the ONE reader thread (upstream `go mux.ReadFromConn(t.ctx)`,
    /// `device_kernel_unix.go:109`). Returns `false` when a reader is already
    /// running (idempotent). The thread holds an `Arc<Shared>`, never an
    /// `Arc<IceMux>`: it is stopped by [`IceMux::stop_reader`] (or by dropping
    /// the mux) and closes nothing itself — the socket's single close is the
    /// owned fd's drop, after the reader has stopped.
    pub fn start_reader(self: &Arc<Self>) -> bool {
        let mut reader = self.reader.lock().unwrap_or_else(|p| p.into_inner());
        if reader.is_some() {
            return false;
        }
        self.shared.reader_stop.store(false, Ordering::Release);
        let shared = Arc::clone(&self.shared);
        let fd = self.fd;
        let spawned = std::thread::Builder::new().name("ice-mux-reader".into()).spawn(move || {
            while !shared.reader_stop.load(Ordering::Acquire) {
                let (ret, _e, rev) = sys::poll1(fd, sys::POLLIN, MUX_READER_POLL_MS as i32);
                if ret < 0 {
                    if sys::errno() == sys::EINTR {
                        continue;
                    }
                    break; // poll itself failed: stop rather than spin
                }
                if ret == 0 {
                    continue; // idle timeout: re-check the stop flag
                }
                if (rev & sys::POLLIN) == 0 {
                    // POLLERR/POLLHUP/POLLNVAL on a datagram socket: nothing
                    // more can be read; stop instead of spinning on a dead fd.
                    break;
                }
                drain_shared(&shared, fd);
            }
        });
        match spawned {
            Ok(handle) => {
                *reader = Some(handle);
                true
            }
            Err(_) => false,
        }
    }

    /// Stop the reader thread and join it. Idempotent; `true` when a running
    /// reader was actually joined. Bounded by [`MUX_READER_POLL_MS`] (the
    /// thread's poll timeout), so a shutdown never waits on a datagram.
    pub fn stop_reader(&self) -> bool {
        let handle = {
            let mut reader = self.reader.lock().unwrap_or_else(|p| p.into_inner());
            self.shared.reader_stop.store(true, Ordering::Release);
            reader.take()
        };
        match handle {
            Some(h) => {
                let _ = h.join();
                true
            }
            None => false,
        }
    }

    /// Whether the reader thread is running right now. False = the caller
    /// drives [`IceMux::drain`] (the injected-clock host harnesses, the ICE
    /// pump's stage-2 cadence).
    pub fn reader_running(&self) -> bool {
        self.reader.lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }

    /// srflx lookup for `server`, upstream `GetXORMappedAddr`
    /// (`universal.go:218-270`): a live cached mapping is returned at once; an
    /// in-flight request is JOINED rather than duplicated; otherwise one
    /// Binding Request is sent and the caller blocks up to `deadline_ms`.
    pub fn get_xor_mapped_addr(
        &self,
        server: MappedAddr,
        deadline_ms: u64,
    ) -> Result<MappedAddr, ManagementError> {
        let rx = self.xor_waiter_or_request(server)?;
        // Wait for the response that fills the entry. Upstream blocks on a
        // channel here while its OWN reader goroutine (`ReadFromConn`,
        // `universal.go:80-108`) keeps filling the cache. When a reader thread
        // runs, that is exactly the situation: wait. When none runs (the
        // injected-clock host harnesses, where a gather runs ON the pump's
        // thread), the waiter drains the socket itself — otherwise it could
        // never be woken. It must NOT drain while the reader runs: a second
        // reader on the same fd would steal datagrams from the reader (and from
        // the WG data plane).
        let started = sys::mono_ms();
        let self_drain = !self.reader_running();
        loop {
            match rx.recv_timeout(Duration::from_millis(5)) {
                Ok(Some(addr)) => return Ok(addr),
                Ok(None) => {
                    return Err(ManagementError::Network("ice-mux:xor-mapped-none".into()))
                }
                // The entry expired (or was replaced) while we waited: upstream
                // closes the waiter channel in exactly that case (`closeWaiters`,
                // `universal.go:229-230`, `:309-319`) and the caller re-tries.
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(ManagementError::Network("ice-mux:xor-mapped-expired".into()));
                }
                Err(RecvTimeoutError::Timeout) => {
                    if sys::mono_ms().saturating_sub(started) >= deadline_ms.max(1) {
                        return Err(ManagementError::Timeout);
                    }
                    if self_drain {
                        self.drain();
                    }
                }
            }
        }
    }

    /// The `GetXORMappedAddr` bookkeeping half: hit ⇒ an already-filled channel
    /// carrying the cached mapping; miss/pending ⇒ our own waiter (a request is
    /// sent for the first waiter only).
    fn xor_waiter_or_request(
        &self,
        server: MappedAddr,
    ) -> Result<Receiver<Option<MappedAddr>>, ManagementError> {
        let now = sys::mono_ms();
        {
            let mut inner = self.lock();
            let expired = inner.xor.get(&server).map(|e| e.expires_at <= now).unwrap_or(false);
            if expired {
                inner.xor.remove(&server); // drops the senders → waiters unblock with Err
                inner.stats.xor_expired += 1;
            }
            if let Some(e) = inner.xor.get_mut(&server) {
                if let Some(addr) = e.addr {
                    inner.stats.xor_hits += 1;
                    let (tx, rx) = std::sync::mpsc::channel();
                    let _ = tx.send(Some(addr));
                    return Ok(rx);
                }
                let (tx, rx) = std::sync::mpsc::channel();
                e.waiters.push(tx);
                return Ok(rx);
            }
        }
        // No entry: we are the one request. The id must exist before the send so
        // a fast response cannot arrive before its entry does.
        let txn = stun::random_transaction_id()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut inner = self.lock();
        if let Some(e) = inner.xor.get_mut(&server) {
            // another thread won the race while we drew entropy
            if let Some(addr) = e.addr {
                let (tx2, rx2) = std::sync::mpsc::channel();
                let _ = tx2.send(Some(addr));
                inner.stats.xor_hits += 1;
                return Ok(rx2);
            }
            e.waiters.push(tx);
            return Ok(rx);
        }
        inner.xor.insert(
            server,
            XorEntry {
                addr: None,
                expires_at: sys::mono_ms().saturating_add(XOR_MAPPED_CACHE_TTL_MS),
                txn: Some(txn),
                waiters: vec![tx],
            },
        );
        let req = stun::build_binding_request(txn);
        let fd = inner.fd.as_raw_fd();
        if !sendto(fd, server, &req) {
            inner.xor.remove(&server);
            return Err(ManagementError::Network(format!(
                "ice-mux:stun-send-failed (errno={})",
                sys::errno()
            )));
        }
        inner.stats.xor_misses += 1;
        Ok(rx)
    }

    /// Test/diagnostics seam: how many waiters are parked on `server`.
    pub fn pending_waiters(&self, server: MappedAddr) -> usize {
        self.lock().xor.get(&server).map(|e| e.waiters.len()).unwrap_or(0)
    }

    /// Test/diagnostics seam: the cached mapping for `server` while it is still
    /// inside its TTL.
    pub fn cached_xor(&self, server: MappedAddr) -> Option<MappedAddr> {
        let now = sys::mono_ms();
        let inner = self.lock();
        inner.xor.get(&server).and_then(|e| if e.expires_at > now { e.addr } else { None })
    }

    /// Test seam: age every cache entry so its TTL is already over (the
    /// injected-clock harnesses never sleep for 25 s).
    pub fn expire_cache_for_test(&self) {
        let now = sys::mono_ms();
        let mut inner = self.lock();
        let stale: Vec<MappedAddr> = inner
            .xor
            .iter()
            .filter(|(_, e)| e.expires_at > now)
            .map(|(k, _)| *k)
            .collect();
        for k in stale {
            inner.xor.remove(&k);
            inner.stats.xor_expired += 1;
        }
    }

    // -----------------------------------------------------------------------
    // classification (upstream HandleSTUNMessage, universal.go:160-184)
    // -----------------------------------------------------------------------
}

impl Drop for IceMux {
    /// Stop the ONE reader thread before the owned socket is released. The fd
    /// itself is closed exactly once, by the `OwnedFd` inside the state — this
    /// drop only makes sure nobody is reading while that happens.
    fn drop(&mut self) {
        self.stop_reader();
    }
}

/// One read pass over the shared socket (see [`IceMux::drain`]). Reads at most
/// [`MUX_DRAIN_BUDGET`] datagrams, classifies each under the state lock, and
/// delivers a non-STUN datagram to the armed sink **after releasing that lock**
/// (the sink enters the WG device; the two locks may never nest this way).
fn drain_shared(shared: &Shared, fd: sys::c_int) -> usize {
    let sink_armed = shared.sink.lock().unwrap_or_else(|p| p.into_inner()).is_some();
    let mut buf = [0u8; MUX_RECV_BUF];
    let mut read = 0usize;
    let mut stun_queued = false;
    while read < MUX_DRAIN_BUDGET {
        let mut src = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        let n = unsafe {
            sys::recvfrom(
                fd,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                buf.len(),
                0,
                &mut src,
                &mut len,
            )
        };
        if n <= 0 {
            break;
        }
        read += 1;
        let from = (src.sin_addr, u16::from_be(src.sin_port));
        let out = {
            let mut inner = shared.st.lock().unwrap_or_else(|p| p.into_inner());
            inner.dispatch(from, &buf[..n as usize], sink_armed)
        };
        if out == DispatchOut::ToSink {
            let sink = shared.sink.lock().unwrap_or_else(|p| p.into_inner()).clone();
            // A sink is armed for the whole life of every production mux (the
            // arm path attaches it before the reader starts) and there is no
            // detach, so `None` here is a test mux that never attached one: the
            // datagram has already been counted in `data_unknown` by `dispatch`
            // (the retired per-session data queue is gone — stage 4).
            if let Some(sink) = sink {
                sink.handle_udp(&buf[..n as usize], from);
            }
        } else if out == DispatchOut::HandledStun {
            stun_queued = true;
        }
    }
    shared.st.lock().unwrap_or_else(|p| p.into_inner()).stats.reads += read as u64;
    // Event-driven ICE pump (0.0.28): the sessions consume their STUN queues in
    // `IceSession::pump_recv`, driven by the pump — so a pass that queued
    // anything must wake it. One wake per drain pass, not per datagram.
    if stun_queued {
        let waker = shared.waker.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(w) = waker {
            w.notify(PumpWake::Mux);
        }
    }
    read
}

/// `true` when `ip` falls inside any of our own overlay networks (upstream
/// `wg.Network.Contains(src)`, `universal.go:169-170`).
fn overlay_contains(overlay: &[OverlayNet], ip: [u8; 4]) -> bool {
    overlay.iter().any(|n| n.contains(ip))
}

impl Inner {
    fn fd(&self) -> sys::c_int {
        self.fd.as_raw_fd()
    }

    fn dispatch(&mut self, from: MappedAddr, datagram: &[u8], sink_armed: bool) -> DispatchOut {
        // 1. the WG-first classifier shared with the session (upstream
        //    `filterOutStunMessages`, ice_bind.go:318-350): anything WG-shaped or
        //    not STUN-shaped belongs to the data plane, and goes STRAIGHT into
        //    the WG device (upstream: WG's own receive path keeps the buffer,
        //    ice_bind.go:257-285, and WG itself drops sources that match no peer
        //    endpoint). The retired per-session data queue is gone (stage 4), so
        //    a sink-less mux drops + counts the datagram instead of queueing it.
        if demux_routes_to_wg(datagram) {
            let id = self.by_addr.get(&from).copied();
            let label = id
                .and_then(|id| self.sessions.get(&id))
                .map(|s| s.peer_label.clone())
                .unwrap_or_else(|| "unknown".into());
            self.note_receipt(from, datagram, &label);
            if id.is_none() || !sink_armed {
                // no session ever registered this source, or no sink to hand the
                // datagram to: it has no consumer. Counted, never queued.
                self.stats.data_unknown += 1;
            }
            return if sink_armed { DispatchOut::ToSink } else { DispatchOut::Handled };
        }
        // 2. STUN: a datagram sourced inside our own overlay can never be a
        //    legitimate STUN message (upstream `HandleSTUNMessage`,
        //    universal.go:168-173 — dropped BEFORE the srflx check and before
        //    the mux forward).
        if overlay_contains(&self.overlay, from.0) {
            self.note_receipt(from, datagram, "unknown");
            self.stats.overlay_dropped += 1;
            return DispatchOut::Handled;
        }
        // 3. a response to OUR srflx query: matched by the server address we have
        //    an entry for, then by the transaction id we sent.
        if let Some(entry_txn) = self.xor.get(&from).and_then(|e| e.txn) {
            if let Ok(stun::StunReply::Mapped { addr, port }) =
                stun::parse_binding_response(datagram, &entry_txn)
            {
                self.note_receipt(from, datagram, "unknown");
                if let Some(e) = self.xor.get_mut(&from) {
                    e.addr = Some((addr, port));
                    for tx in e.waiters.drain(..) {
                        let _ = tx.send(Some((addr, port)));
                    }
                }
                self.stats.xor_filled += 1;
                return DispatchOut::Handled;
            }
        }
        // 4. a session's check/keepalive: Binding REQUESTS route by the ufrag in
        //    USERNAME (upstream `findConnectionByUsername`, mux.go:455-468),
        //    everything else by the remote address it came from
        //    (`forwardToAllConnections` + `addressMap`, mux.go:425-452).
        let id = if msg_type(datagram) == Some(BINDING_REQUEST) {
            username_ufrag(datagram)
                .and_then(|u| self.by_ufrag.get(u).copied())
                .or_else(|| self.by_addr.get(&from).copied())
        } else {
            self.by_addr.get(&from).copied()
        };
        match id {
            Some(id) => {
                let label = self
                    .sessions
                    .get(&id)
                    .map(|s| s.peer_label.clone())
                    .unwrap_or_else(|| "unknown".into());
                self.note_receipt(from, datagram, &label);
                return if self.push_stun(id, from, datagram) {
                    DispatchOut::HandledStun
                } else {
                    // Queue full (a stalled consumer): dropped + counted, and
                    // there is nothing new for the pump to come and take.
                    DispatchOut::Handled
                };
            }
            None => {
                self.note_receipt(from, datagram, "unknown");
                self.stats.stun_unknown += 1;
            }
        }
        DispatchOut::Handled
    }

    /// Queue one inbound STUN datagram for its session. Returns `false` when the
    /// per-session queue is at [`MUX_STUN_QCAP`] (dropped + counted) — the
    /// caller then must not claim a delivery to wake the pump for.
    fn push_stun(&mut self, id: u64, src: MappedAddr, datagram: &[u8]) -> bool {
        let Some(s) = self.sessions.get_mut(&id) else { return false };
        if s.stun.len() >= MUX_STUN_QCAP {
            self.stats.queue_dropped += 1;
            return false;
        }
        s.stun.push_back(MuxStun { src, bytes: datagram.to_vec() });
        true
    }

    /// N2 v4.1 P7: one outer-source receipt per datagram read on the shared
    /// socket (formerly the per-session socket's own drain, now this reader —
    /// see `n2_outer_receipt`'s "seams" list). Unarmed this is one atomic load.
    fn note_receipt(&self, from: MappedAddr, datagram: &[u8], peer: &str) {
        if !crate::n2_outer_receipt::armed() {
            return;
        }
        let local = crate::n2_socket_trace::local_addr(self.fd());
        crate::n2_outer_receipt::note_udp(
            local,
            std::net::SocketAddr::from((
                std::net::Ipv4Addr::from(from.0),
                from.1,
            )),
            datagram,
            peer,
        );
    }
}

/// `stun.Message` message type from the first two bytes (the mux needs only
/// the type; the full decoder lives in `ice_session`/`stun`).
fn msg_type(datagram: &[u8]) -> Option<u16> {
    if datagram.len() < stun::HEADER_LEN {
        return None;
    }
    Some(u16::from_be_bytes([datagram[0], datagram[1]]))
}

/// The first field of the USERNAME attribute (`"<local-ufrag>:<remote-ufrag>"`
/// on a Binding Request, RFC 8445 §7.2.1.1) — upstream
/// `strings.Split(string(attr), ":")[0]` (`mux.go:461`).
fn username_ufrag(datagram: &[u8]) -> Option<&str> {
    const ATTR_USERNAME: u16 = 0x0006;
    if datagram.len() < stun::HEADER_LEN {
        return None;
    }
    let mlen = u16::from_be_bytes([datagram[2], datagram[3]]) as usize;
    let body = datagram.get(stun::HEADER_LEN..stun::HEADER_LEN + mlen)?;
    let mut off = 0usize;
    while off + 4 <= body.len() {
        let atype = u16::from_be_bytes([body[off], body[off + 1]]);
        let alen = u16::from_be_bytes([body[off + 2], body[off + 3]]) as usize;
        off += 4;
        if off + alen > body.len() {
            return None;
        }
        if atype == ATTR_USERNAME {
            return core::str::from_utf8(&body[off..off + alen]).ok()?.split(':').next();
        }
        off += (alen + 3) & !3;
    }
    None
}

/// `sendto` with the EINTR retry idiom every other UDP path in this crate uses.
fn sendto(fd: sys::c_int, dst: MappedAddr, buf: &[u8]) -> bool {
    let addr = sys::sockaddr_in::new(dst.0, dst.1);
    for _ in 0..3 {
        let n = unsafe {
            sys::sendto(
                fd,
                buf.as_ptr() as *const core::ffi::c_void,
                buf.len(),
                0,
                &addr,
                core::mem::size_of::<sys::sockaddr_in>() as u32,
            )
        };
        if n >= 0 {
            return true;
        }
        if sys::errno() != sys::EINTR {
            return false;
        }
    }
    false
}

/// Create a UDP socket and bind it to the WILDCARD address on `port` (`0` =
/// ephemeral). HOST/TEST only: on device the one socket is the WG outer socket
/// (`wg_fwd_open`, opened and bound wildcard exactly like this) and the mux is
/// built over a dup of it. Returns the RAW fd (ownership crosses to the caller)
/// and the bound port; on failure the socket is closed again — there is no
/// unbound fallback.
fn open_bound_wildcard(port: u16) -> Result<(sys::c_int, u16), ManagementError> {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(ManagementError::Network(format!(
            "ice-mux:socket-failed (errno={})",
            sys::errno()
        )));
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let bind_sa = sys::sockaddr_in::new([0, 0, 0, 0], port);
    if unsafe { sys::bind(fd, &bind_sa, core::mem::size_of::<sys::sockaddr_in>() as u32) } == -1 {
        let e = sys::errno();
        drop(owned); // the single close on the failure path
        return Err(ManagementError::Network(format!("ice-mux:bind-failed (errno={e})")));
    }
    let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(fd, &mut name, &mut len) } != 0 {
        let e = sys::errno();
        drop(owned);
        return Err(ManagementError::Network(format!("ice-mux:getsockname-failed (errno={e})")));
    }
    Ok((owned.into_raw_fd(), u16::from_be(name.sin_port)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ice_session::ATTR_USERNAME;

    /// `[u8; 4]` of an IPv4 `IpAddr` (tests only speak UDP4).
    fn octets(ip: std::net::IpAddr) -> [u8; 4] {
        match ip {
            std::net::IpAddr::V4(v) => v.octets(),
            std::net::IpAddr::V6(_) => [0, 0, 0, 0],
        }
    }

    /// Build a STUN message of `mtype` with the given USERNAME (no integrity —
    /// the mux routes on USERNAME and the type only; the session authenticates).
    fn stun_message(mtype: u16, username: &str) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.extend_from_slice(&mtype.to_be_bytes());
        let mut body = Vec::new();
        body.extend_from_slice(&ATTR_USERNAME.to_be_bytes());
        body.extend_from_slice(&(username.len() as u16).to_be_bytes());
        body.extend_from_slice(username.as_bytes());
        while body.len() % 4 != 0 {
            body.push(0);
        }
        msg.extend_from_slice(&(body.len() as u16).to_be_bytes());
        msg.extend_from_slice(&stun::MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(&[7u8; 12]);
        msg.extend_from_slice(&body);
        msg
    }

    fn binding_request_with_username(username: &str) -> Vec<u8> {
        stun_message(stun::BINDING_REQUEST, username)
    }

    fn mux() -> Arc<IceMux> {
        IceMux::open_wildcard(0).expect("mux socket")
    }

    fn loopback_sender(sender: &std::net::UdpSocket, mux_port: u16, bytes: &[u8]) {
        sender
            .send_to(bytes, ("127.0.0.1", mux_port))
            .expect("send to mux");
    }

    fn recv_until(mux: &IceMux, id: u64, want: usize) -> Vec<MuxStun> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let got = mux.take_stun(id, 8);
            if got.len() >= want || std::time::Instant::now() > deadline {
                return got;
            }
            mux.drain();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn binding_request_routes_to_the_session_that_owns_the_ufrag() {
        let mux = mux();
        mux.register_session(1, "AAAA", "peer-a");
        mux.register_session(2, "BBBB", "peer-b");
        let port = mux.local_port();
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        // RFC 8445 §7.2.1.1: "<their-ufrag>:<our-ufrag>" from the sender's view,
        // i.e. OUR ufrag first — which is what upstream splits on.
        loopback_sender(&sender, port, &binding_request_with_username("AAAA:ZZZZ"));
        let got = recv_until(&mux, 1, 1);
        assert_eq!(got.len(), 1, "session 1 takes the request");
        assert_eq!(got[0].src.0, [127, 0, 0, 1]);
        assert_eq!(got[0].src.1, sender.local_addr().unwrap().port());
        assert!(mux.take_stun(2, 8).is_empty(), "session 2 must not see it");
    }

    #[test]
    fn response_without_username_routes_by_remote_address_learned_from_a_send() {
        let mux = mux();
        mux.register_session(7, "AAAA", "peer-a");
        let peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer_addr = peer.local_addr().unwrap();
        // the session writes first: that is what registers the address
        let sent = mux.send_to(7, (octets(peer_addr.ip()), peer_addr.port()), b"hello");
        assert!(sent, "send on the shared socket");
        assert_eq!(octets(peer_addr.ip()), [127, 0, 0, 1]);
        // a response (a Binding Success carries no USERNAME) from that address
        // comes back to session 7
        let reply = stun_message(stun::BINDING_SUCCESS, "");
        peer.send_to(&reply, ("127.0.0.1", mux.local_port())).unwrap();
        let got = recv_until(&mux, 7, 1);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].src.1, peer_addr.port());
    }

    /// 0.0.28 event-driven ICE pump: a drain pass that queued STUN for a session
    /// wakes the pump EXACTLY once per pass, and packets with no session
    /// consumer (dropped/cached/routed to WG) never wake it. Without this the
    /// session's queue would only be read at the next coarse deadline.
    #[test]
    fn queued_stun_wakes_the_ice_pump_once_per_drain_pass() {
        let mux = mux();
        let waker = crate::peer_conn::PumpWaker::new();
        mux.set_waker(waker.clone());
        mux.register_session(1, "AAAA", "peer-a");
        let port = mux.local_port();
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let wakes = || waker.counts()[crate::peer_conn::PumpWake::Mux.index()];

        // Non-STUN (no sink armed → dropped + counted) and an unknown ufrag:
        // nothing for the ICE pump to come and take.
        loopback_sender(&sender, port, b"not-a-stun-datagram-at-all");
        loopback_sender(&sender, port, &binding_request_with_username("NOPE:ZZZZ"));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while mux.stats().stun_unknown + mux.stats().data_unknown < 2
            && std::time::Instant::now() < deadline
        {
            mux.drain();
        }
        assert_eq!(wakes(), 0, "a datagram no session can take must not wake the pump");

        // A real Binding Request for the session: one wake on the pass that
        // queued it (and only that pass — an empty pass does not notify).
        let gen_before = waker.generation();
        loopback_sender(&sender, port, &binding_request_with_username("AAAA:ZZZZ"));
        let got = recv_until(&mux, 1, 1);
        assert_eq!(got.len(), 1);
        assert_eq!(wakes(), 1, "exactly one wake for the pass that queued the frame");
        assert!(waker.generation() > gen_before, "the queued frame moved the generation");
    }

    #[test]
    fn unknown_packets_are_dropped_and_counted() {
        let mux = mux();
        mux.register_session(1, "AAAA", "peer-a");
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        // STUN with an unknown ufrag and no address registration
        loopback_sender(&sender, mux.local_port(), &binding_request_with_username("NOPE:ZZZZ"));
        // non-STUN from an unknown address
        loopback_sender(&sender, mux.local_port(), b"not-a-stun-datagram-at-all");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while mux.stats().stun_unknown + mux.stats().data_unknown < 2
            && std::time::Instant::now() < deadline
        {
            mux.drain();
        }
        let s = mux.stats();
        assert_eq!(s.stun_unknown, 1, "unknown ufrag is dropped, not broadcast");
        assert_eq!(s.data_unknown, 1, "unknown data source is dropped");
        assert!(mux.take_stun(1, 8).is_empty(), "nothing may reach the session");
    }

    #[test]
    fn dropping_a_session_never_closes_the_shared_socket() {
        let mux = mux();
        mux.register_session(1, "AAAA", "peer-a");
        let port = mux.local_port();
        mux.unregister_session(1);
        assert_eq!(mux.session_count(), 0);
        // the fd is still ours and still usable
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        mux.register_session(2, "BBBB", "peer-b");
        loopback_sender(&sender, port, &binding_request_with_username("BBBB:ZZZZ"));
        assert_eq!(recv_until(&mux, 2, 1).len(), 1, "the mux survived the session drop");
    }

    /// A stand-in for the production reader (or the host-path ICE pump, which
    /// drains every `crate::connector::ICE_PUMP_TICK_MS` while no reader
    /// thread runs), so a unit test that expects a srflx response to be
    /// handled must read too.
    struct Reader {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Reader {
        fn start(mux: Arc<IceMux>) -> Reader {
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = stop.clone();
            let handle = std::thread::spawn(move || {
                while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                    mux.drain();
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            Reader { stop, handle: Some(handle) }
        }
    }

    impl Drop for Reader {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    #[test]
    fn srflx_response_fills_the_cache_and_wakes_every_waiter() {
        let mux = mux();
        let _reader = Reader::start(mux.clone());
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = (
            [127u8, 0, 0, 1],
            server_addr_port(&server),
        );
        // one requester in flight
        let m = mux.clone();
        let waiting = std::thread::spawn(move || m.get_xor_mapped_addr(server_addr, 3000));
        // let the request reach the fake server, then answer with XOR-MAPPED
        let (req, from) = recv_from(&server);
        assert_eq!(req.len(), stun::REQUEST_LEN);
        assert_eq!(mux.pending_waiters(server_addr), 1);
        server
            .send_to(&xor_success(&req, [203, 0, 113, 7], 4242), from)
            .unwrap();
        assert_eq!(waiting.join().unwrap().unwrap(), ([203, 0, 113, 7], 4242));
        assert_eq!(mux.cached_xor(server_addr), Some(([203, 0, 113, 7], 4242)));
        // a second caller is served from the cache without a new request
        let before = mux.stats().xor_misses;
        assert_eq!(
            mux.get_xor_mapped_addr(server_addr, 1000).unwrap(),
            ([203, 0, 113, 7], 4242)
        );
        assert_eq!(mux.stats().xor_misses, before, "cache hit sends nothing");
        assert_eq!(mux.stats().xor_hits, 1);
    }

    #[test]
    fn in_flight_requests_for_one_server_are_deduplicated() {
        let mux = mux();
        let _reader = Reader::start(mux.clone());
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = ([127u8, 0, 0, 1], server_addr_port(&server));
        let mut waiters = Vec::new();
        for _ in 0..4 {
            let m = mux.clone();
            waiters.push(std::thread::spawn(move || m.get_xor_mapped_addr(server_addr, 3000)));
        }
        // exactly ONE Binding Request leaves the mux, and four callers wait
        let (req, from) = recv_from(&server);
        server
            .send_to(&xor_success(&req, [198, 51, 100, 9], 5555), from)
            .unwrap();
        for w in waiters {
            assert_eq!(w.join().unwrap().unwrap(), ([198, 51, 100, 9], 5555));
        }
        assert_eq!(mux.stats().xor_misses, 1, "one STUN round for four agents");
    }

    #[test]
    fn an_expired_entry_is_re_queried_and_its_old_waiters_are_released() {
        let mux = mux();
        let _reader = Reader::start(mux.clone());
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = ([127u8, 0, 0, 1], server_addr_port(&server));
        let m = mux.clone();
        let first = std::thread::spawn(move || m.get_xor_mapped_addr(server_addr, 3000));
        let (req, from) = recv_from(&server);
        server.send_to(&xor_success(&req, [203, 0, 113, 7], 4242), from).unwrap();
        assert!(first.join().unwrap().is_ok());
        mux.expire_cache_for_test();
        assert_eq!(mux.cached_xor(server_addr), None, "TTL expired");
        // the next caller re-queries: a SECOND request appears on the server
        let m2 = mux.clone();
        let second = std::thread::spawn(move || m2.get_xor_mapped_addr(server_addr, 3000));
        let (req2, from2) = recv_from(&server);
        assert_ne!(req2[8..20], req[8..20], "a fresh transaction id");
        server
            .send_to(&xor_success(&req2, [203, 0, 113, 8], 4243), from2)
            .unwrap();
        assert_eq!(second.join().unwrap().unwrap(), ([203, 0, 113, 8], 4243));
        assert_eq!(mux.stats().xor_expired, 1);
        assert_eq!(mux.stats().xor_misses, 2);
    }

    #[test]
    fn a_waiting_caller_times_out_with_timeout_not_a_mapping() {
        let mux = mux();
        let server_addr = ([127u8, 0, 0, 1], black_hole_port());
        let started = std::time::Instant::now();
        let err = mux.get_xor_mapped_addr(server_addr, 150).expect_err("no answer");
        assert!(matches!(err, ManagementError::Timeout), "got {err:?}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn stun_from_inside_our_own_overlay_is_dropped() {
        let sock = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        let fd = unsafe {
            OwnedFd::from_raw_fd(std::os::fd::IntoRawFd::into_raw_fd(sock))
        };
        let mux = IceMux::adopt(
            fd,
            vec![OverlayNet { addr: [100, 108, 0, 0], prefix: 16 }],
            0,
        )
        .expect("mux");
        mux.register_session(1, "AAAA", "peer-a");
        // A session-shaped request from an overlay source must NOT be routed,
        // even though the ufrag matches (upstream universal.go:168-173).
        // Loopback cannot carry 100.108.x.x, so the datagram is dispatched
        // directly through the same classifier the reader uses.
        let raw = binding_request_with_username("AAAA:ZZZZ");
        mux.lock().dispatch(([100, 108, 144, 165], 5000), &raw, false);
        assert_eq!(mux.stats().overlay_dropped, 1);
        assert!(mux.take_stun(1, 8).is_empty(), "overlay-sourced STUN never reaches a session");
    }

    #[test]
    fn host_candidates_are_the_mux_port_times_every_allowed_interface() {
        let mux = mux();
        let port = mux.local_port();
        let ifaces = vec![
            crate::ice::InterfaceAddr { name: "wlan0".into(), addr: [192, 168, 1, 5] },
            crate::ice::InterfaceAddr { name: "chba0".into(), addr: [192, 168, 1, 6] },
        ];
        let cands = crate::ice::mux_host_candidates(&ifaces, port);
        assert_eq!(cands.len(), 2);
        assert!(cands.iter().all(|c| c.port == port), "ONE port for every candidate");
        assert_eq!(cands[0].address, "192.168.1.5");
        assert_eq!(cands[1].address, "192.168.1.6");
    }

    // --- stage 3: the reader thread, the WG sink, fd lifetime ---------------

    /// A WG data-plane stand-in: records every non-STUN datagram the mux handed
    /// it, exactly the tuple `WgDevice::handle_udp` receives.
    #[derive(Default)]
    struct RecordingSink {
        got: Mutex<Vec<(MappedAddr, Vec<u8>)>>,
    }

    impl RecordingSink {
        fn count(&self) -> usize {
            self.got.lock().unwrap().len()
        }
        fn first(&self) -> Option<(MappedAddr, Vec<u8>)> {
            self.got.lock().unwrap().first().cloned()
        }
    }

    impl MuxUdpSink for RecordingSink {
        fn handle_udp(&self, datagram: &[u8], src: MappedAddr) {
            self.got.lock().unwrap().push((src, datagram.to_vec()));
        }
    }

    /// A WG-shaped datagram (`isWireGuardMsg`: type 4 in the low byte, three
    /// zero bytes, >= 32 bytes) with `tag` in the payload so a test can tell two
    /// of them apart.
    fn wg_datagram(tag: u8) -> Vec<u8> {
        let mut pkt = vec![0u8; 64];
        pkt[0] = 4; // WG transport message type
        pkt[40] = tag;
        pkt
    }

    #[test]
    fn the_reader_thread_delivers_non_stun_straight_to_the_wg_sink() {
        let mux = mux();
        let sink = Arc::new(RecordingSink::default());
        assert!(!mux.attach_udp_sink(sink.clone()), "first attach");
        assert!(mux.sink_armed());
        assert!(mux.start_reader(), "the ONE reader thread starts");
        assert!(!mux.start_reader(), "…and starting it twice is a no-op");
        assert!(mux.reader_running());
        // a session exists (so the mux knows the source is ours), and the peer
        // address becomes known by the session writing first — the upstream
        // `addressMap` rule. The datagram itself is DELIVERED regardless of the
        // registry now: WG owns the endpoint match (`ice_bind.go:257-285`).
        mux.register_session(1, "AAAA", "peer-a");
        let peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer_addr = peer.local_addr().unwrap();
        mux.send_to(1, (octets(peer_addr.ip()), peer_addr.port()), b"hello");
        let wire = wg_datagram(7);
        peer.send_to(&wire, ("127.0.0.1", mux.local_port())).unwrap();
        // the reader is a real thread on the real clock: give it a bounded window
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while sink.count() == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        let (src, bytes) = sink.first().expect("the reader thread handed the WG datagram on");
        assert_eq!(src.1, peer_addr.port());
        assert_eq!(bytes, wire);
        // …and the STUN side is untouched: a Binding Request still lands in the
        // session queue (both halves of the demux over ONE reader)
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        loopback_sender(&sender, mux.local_port(), &binding_request_with_username("AAAA:ZZZZ"));
        assert_eq!(recv_until(&mux, 1, 1).len(), 1, "STUN still reaches its session");
        assert!(mux.stop_reader(), "the reader is joined");
        assert!(!mux.reader_running());
    }

    #[test]
    fn a_srflx_waiter_is_woken_by_the_reader_thread_alone() {
        // With a reader running, `get_xor_mapped_addr` must NOT drain the socket
        // itself (a second reader would steal datagrams from the first): only the
        // reader can fill the cache and wake the waiter. If the waiter were still
        // draining, this test would pass either way — so also pin the inverse
        // below with no reader.
        let mux = mux();
        assert!(mux.start_reader());
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = ([127u8, 0, 0, 1], server_addr_port(&server));
        let m = mux.clone();
        let waiting = std::thread::spawn(move || m.get_xor_mapped_addr(server_addr, 3000));
        let (req, from) = recv_from(&server);
        assert_eq!(req.len(), stun::REQUEST_LEN);
        server.send_to(&xor_success(&req, [198, 51, 100, 9], 5555), from).unwrap();
        assert_eq!(waiting.join().unwrap().unwrap(), ([198, 51, 100, 9], 5555));
        assert_eq!(mux.cached_xor(server_addr), Some(([198, 51, 100, 9], 5555)));
        assert!(mux.stop_reader());
    }

    #[test]
    fn a_mux_without_a_sink_drops_non_stun_and_counts_it() {
        // STAGE 4 (official-alignment audit §2.7): the retired per-session data
        // queue is gone, so a mux that never attached a sink has nowhere to hand
        // a non-STUN datagram — it is dropped AND counted (`data_unknown`), never
        // queued and never silently lost. Production always arms the sink before
        // the ONE reader starts.
        let mux = mux();
        mux.register_session(3, "AAAA", "peer-a");
        let peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer_addr = peer.local_addr().unwrap();
        mux.send_to(3, ([127, 0, 0, 1], peer_addr.port()), b"first");
        peer.send_to(&wg_datagram(1), ("127.0.0.1", mux.local_port())).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while mux.stats().data_unknown == 0 && std::time::Instant::now() < deadline {
            mux.drain();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(mux.stats().data_unknown, 1, "the sink-less drop is counted");
        assert!(mux.take_stun(3, 8).is_empty(), "WG data is not STUN");
    }

    #[test]
    fn a_full_session_queue_drops_and_counts_without_stopping_the_reader() {
        // BACK-PRESSURE: the queue is bounded and drops the NEWEST with a counter
        // (never unbounded growth, never a blocked reader); the socket keeps being
        // read and the NEXT session still gets served on the same pass sequence.
        let mux = mux();
        assert!(mux.start_reader());
        mux.register_session(1, "AAAA", "stalled-consumer");
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let extra = 10usize;
        let total = (MUX_STUN_QCAP + extra) as u64;
        for _ in 0..total {
            loopback_sender(&sender, mux.local_port(), &binding_request_with_username("AAAA:ZZZZ"));
        }
        // the stalled session never drains: the reader must count the overflow and
        // keep going
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while mux.stats().queue_dropped < extra as u64 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        // `queue_dropped` is bumped per datagram INSIDE a drain pass, but `reads`
        // is published once at the END of that pass. The overflow total can be
        // visible one pass before `reads` catches up (observed: dropped=10 with
        // reads=73, the 74th already read), so wait for the pass to publish its
        // count before asserting — a half-counted pass is not a lost datagram.
        while mux.stats().reads < total && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        let s = mux.stats();
        assert_eq!(s.queue_dropped, extra as u64, "exactly the overflow is dropped+counted");
        assert!(s.reads >= total, "every datagram was still read");
        assert_eq!(mux.take_stun(1, usize::MAX).len(), MUX_STUN_QCAP, "queue stayed bounded");
        // a SECOND session on the same socket is served normally afterwards
        mux.register_session(2, "BBBB", "live-consumer");
        let d2 = std::time::Instant::now() + Duration::from_secs(3);
        loopback_sender(&sender, mux.local_port(), &binding_request_with_username("BBBB:ZZZZ"));
        loop {
            if !mux.take_stun(2, 8).is_empty() {
                break;
            }
            assert!(std::time::Instant::now() < d2, "the reader survived the overflow");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(mux.stop_reader());
    }

    #[test]
    fn dropping_the_last_mux_closes_its_own_fd_exactly_once_and_leaves_the_socket() {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let raw = std::os::fd::IntoRawFd::into_raw_fd(sock); // the mux takes it over
        // Move the socket OUT of the concurrent-allocator range (the same fix
        // `host_sockets::tests::fd_bag_closes_every_kept_fd` documents): fd
        // numbers are handed out lowest-free-first, so a number opened here can
        // be recycled by a parallel test between the mux's drop and the EBADF
        // probe — that is the observed flake. A high number is never handed out
        // by those tests, so the post-drop probe is race-free.
        let high = [1 << 20, 1 << 18, 1 << 16, 1 << 14]
            .into_iter()
            .find_map(|floor| {
                let hi = unsafe { sys::fcntl(raw, sys::F_DUPFD_CLOEXEC, floor) };
                (hi >= floor).then_some(hi)
            })
            .unwrap_or_else(|| {
                panic!("no high-fd tier worked for fd {raw}: race-free EBADF assertion impossible")
            });
        unsafe { sys::close(raw) };
        // a second dup of the SAME socket keeps the underlying socket alive
        let keep = crate::mgmtsock::dup_socket_fd(high).expect("dup for the assertion");
        let owned: OwnedFd = unsafe { OwnedFd::from_raw_fd(high) };
        let mux = IceMux::adopt(owned, vec![], 0).expect("mux over the bound socket");
        assert!(mux.start_reader());
        assert_ne!(mux.local_port(), 0);
        drop(mux); // stops the reader, then closes exactly its own fd

        let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        assert_eq!(
            unsafe { sys::getsockname(high, &mut name, &mut len) },
            -1,
            "the mux's own fd is closed when the last Arc goes"
        );
        assert_eq!(sys::errno(), sys::EBADF);
        // the socket itself (our dup) is untouched — the mux never closed it
        let mut name2 = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut len2 = core::mem::size_of::<sys::sockaddr_in>() as u32;
        assert_eq!(
            unsafe { sys::getsockname(keep, &mut name2, &mut len2) },
            0,
            "only the mux's OWN dup was closed"
        );
        unsafe { sys::close(keep) };
    }

    #[test]
    fn stop_reader_is_bounded_and_does_nothing_without_a_reader() {
        let mux = mux();
        assert!(!mux.stop_reader(), "no reader: nothing to join");
        assert!(mux.start_reader());
        let started = std::time::Instant::now();
        assert!(mux.stop_reader(), "the reader joined");
        assert!(
            started.elapsed() < Duration::from_millis(MUX_READER_POLL_MS * 4 + 500),
            "shutdown is bounded by the poll timeout, took {:?}",
            started.elapsed()
        );
        assert!(!mux.stop_reader(), "idempotent");
        // restarting is allowed (a replacement mux generation)
        assert!(mux.start_reader());
        assert!(mux.stop_reader());
    }

    // --- fixtures -----------------------------------------------------------

    fn server_addr_port(sock: &std::net::UdpSocket) -> u16 {
        sock.local_addr().unwrap().port()
    }

    fn recv_from(sock: &std::net::UdpSocket) -> (Vec<u8>, std::net::SocketAddr) {
        sock.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut buf = [0u8; 512];
        let (n, from) = sock.recv_from(&mut buf).expect("stun request");
        (buf[..n].to_vec(), from)
    }

    /// A Binding Success carrying XOR-MAPPED-ADDRESS for `req`'s transaction.
    fn xor_success(req: &[u8], addr: [u8; 4], port: u16) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.extend_from_slice(&stun::BINDING_SUCCESS.to_be_bytes());
        let mut body = Vec::new();
        body.extend_from_slice(&stun::ATTR_XOR_MAPPED.to_be_bytes());
        body.extend_from_slice(&8u16.to_be_bytes());
        let cookie = stun::MAGIC_COOKIE;
        body.push(0x00);
        body.push(0x01);
        body.extend_from_slice(&(port ^ (cookie >> 16) as u16).to_be_bytes());
        for (i, b) in addr.iter().enumerate() {
            body.push(b ^ (cookie >> (24 - 8 * i)) as u8);
        }
        msg.extend_from_slice(&(body.len() as u16).to_be_bytes());
        msg.extend_from_slice(&cookie.to_be_bytes());
        msg.extend_from_slice(&req[8..20]);
        msg.extend_from_slice(&body);
        msg
    }

    /// A port nobody answers on: bind, read it, close it (the OS will not hand
    /// the same number out again inside this test).
    fn black_hole_port() -> u16 {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let p = s.local_addr().unwrap().port();
        drop(s);
        p
    }
}
