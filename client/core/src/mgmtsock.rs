// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # mgmtsock — protected management socket seam (N3-7)
//!
//! Closes the first fail-closed gap: the management connection's own socket
//! never went through a protect gate, so once the VPN owns the default route
//! the management dial is eaten by our own tunnel (bootstrap loop) — a
//! violation of governance `docs/native-nx-governance.md` §二 item 4
//! ("建连前逐 socket protect … protect 失败 fail-closed", enumerated sockets
//! include management, and protection is required per FIRST connect AND per
//! reconnect; an API return value is not evidence).
//!
//! ## Upstream semantics (pinned commit `791401060d2b`, file:line only)
//!
//! - `ControlProtectSocket` (`client/net/protectsocket_android.go:22-46`) is
//!   installed as the `Dialer.Control` hook (`client/net/dialer_init_android.go:4-6`),
//!   i.e. it runs INSIDE every dial, BEFORE `connect(2)`, on the raw socket
//!   fd. A missing protector (`androidProtectSocket == nil`, L36-38) or a
//!   failed platform protect (`!androidProtectSocket(fd)`, L40-42) makes the
//!   CONTROL function return an error, which FAILS THE DIAL — upstream is
//!   fail-closed by construction, and so is this seam.
//! - The management retry loop wraps the whole dial+login+stream
//!   (`shared/management/client/grpc.go:224-275`), so EVERY reconnect
//!   re-dials and therefore re-protects. There is no "protect once" mode.
//!
//! ## fd contract (this module's part of governance §二 item 2)
//!
//! Production feeds duplicate the shell-owned fd immediately and retain the
//! copy in the provider queue. The shell closes its original after feed;
//! subsequent dial takes ownership of the queued copy until connect/adoption.
//! The provider never reads, writes or closes the shell's original fd. TUN
//! ownership is separate: its original remains with `VpnConnection.destroy()`.
//!
//! A duplicate shares the open-file description, so `O_NONBLOCK` set at dial
//! also affects other surviving references to the same socket. Providers
//! must not do blocking I/O on a socket after handing it over.
//!
//! ## Per-dial re-acquire (never reuse an old fd)
//!
//! [`ProtectedSocketConnector`] (the tonic `Service<Uri>`) takes a FRESH
//! socket from [`ManagementSocketProvider`] on EVERY invocation — the
//! initial dial and every hyper/tonic re-dial of the channel alike. A
//! consumed TCP socket can never serve a second dial (its connection state
//! is dead), so reuse is impossible by construction, and an empty provider
//! fails the dial (fail-closed) instead of falling back to an unprotected
//! direct connection.

use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use hyper_util::rt::TokioIo;
use tonic::transport::Uri;
use tower_service::Service;

use crate::hilog;
use crate::sys;

// ---------------------------------------------------------------------------
// provider seam
// ---------------------------------------------------------------------------

/// Why the seam could not hand out a protected socket (stable tokens only —
/// these cross the NAPI error surface, so no OS strings, no secrets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketSeamError {
    /// Provider has no protected socket right now (queue empty / none
    /// provided). Fail-closed: the dial fails, nothing dials unprotected.
    NoSocket,
    /// The provided fd number is not an open descriptor (dup/F_GETFD failed).
    BadFd { errno: i32 },
    /// `F_SETFL(O_NONBLOCK)` failed on the dup copy.
    NonBlock { errno: i32 },
    Supply(&'static str),
}

impl SocketSeamError {
    /// Stable error token (the only text that may cross the boundary).
    pub fn token(&self) -> &'static str {
        match self {
            SocketSeamError::NoSocket => "no-protected-socket",
            SocketSeamError::BadFd { .. } => "socket-fd-invalid",
            SocketSeamError::NonBlock { .. } => "socket-nonblock-failed",
            SocketSeamError::Supply(reason) => reason,
        }
    }

    pub fn errno(&self) -> i32 {
        match self {
            SocketSeamError::NoSocket | SocketSeamError::Supply(_) => 0,
            SocketSeamError::BadFd { errno } | SocketSeamError::NonBlock { errno } => *errno,
        }
    }
}

impl core::fmt::Display for SocketSeamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} (errno={})", self.token(), self.errno())
    }
}

impl std::error::Error for SocketSeamError {}

/// Source of pre-protected management TCP sockets. ONE call = ONE socket,
/// handed out exactly once; a fresh dial needs a fresh socket (upstream
/// re-protects per dial, `grpc.go:224-275`).
///
/// Contract: the returned fd number is a BORROWED NUMBER — the consumer dups
/// it and never touches the original (module fd contract above). `Err` means
/// "no protected socket available": the caller must fail the dial.
pub trait ManagementSocketProvider: Send + Sync + 'static {
    fn take_fd(&self) -> Result<i32, SocketSeamError>;

    fn take_for_dial(&self) -> Pin<Box<dyn Future<Output = Result<SocketLease, SocketSeamError>> + Send + '_>> {
        Box::pin(async { self.take_fd().map(SocketLease::Borrowed) })
    }
}

pub enum SocketLease {
    Borrowed(i32),
    Owned(OwnedFd),
}

impl SocketLease {
    fn fd(&self) -> i32 {
        match self {
            Self::Borrowed(fd) => *fd,
            Self::Owned(fd) => fd.as_raw_fd(),
        }
    }
}

/// Production provider: a FIFO queue of protected sockets seeded by
/// `connector_start_with_socket` (first socket) and refilled by the shell
/// via `connector_socket_feed` (fresh open+protect per socket). Empty queue
/// → `Err(NoSocket)` — fail-closed; reconnect dials keep failing until the
/// shell feeds a fresh protected socket (see the notes doc for the policy).
#[derive(Debug)]
pub struct ProtectedSocketFdSource {
    queue: Mutex<VecDeque<OwnedFd>>,
    taken: AtomicU64,
    supply: std::sync::OnceLock<(Arc<crate::socket_supply::SocketSupply>, crate::socket_supply::Purpose)>,
}

impl ProtectedSocketFdSource {
    /// Seed with the first protected socket.
    pub fn new_with_fd(fd: i32) -> Result<Self, SocketSeamError> {
        let mut q = VecDeque::with_capacity(2);
        if fd >= 0 {
            q.push_back(unsafe { OwnedFd::from_raw_fd(dup_socket_fd(fd)?) });
        }
        Ok(ProtectedSocketFdSource { queue: Mutex::new(q), taken: AtomicU64::new(0), supply: std::sync::OnceLock::new() })
    }

    pub fn attach_supply(&self, supply: Arc<crate::socket_supply::SocketSupply>, purpose: crate::socket_supply::Purpose) {
        let _ = self.supply.set((supply, purpose));
    }

    pub fn socket_supply(&self) -> Option<Arc<crate::socket_supply::SocketSupply>> {
        self.supply.get().map(|(supply, _)| supply.clone())
    }

    /// Shell-side resupply: retain an owned duplicate immediately.
    pub fn feed(&self, fd: i32) -> Result<(), SocketSeamError> {
        let owned = unsafe { OwnedFd::from_raw_fd(dup_socket_fd(fd)?) };
        self.queue.lock().expect("fd queue").push_back(owned);
        Ok(())
    }

    /// Drop unconsumed owned copies after their physical network is lost.
    pub fn clear_pending(&self) {
        self.queue.lock().expect("fd queue").clear();
    }

    /// Sockets currently queued (observability; crosses into
    /// `connector_socket_feed` JSON as `queued`).
    pub fn pending(&self) -> usize {
        self.queue.lock().expect("fd queue").len()
    }

    /// How many sockets this source handed out (observability; the tests
    /// pin it to the number of connection attempts).
    pub fn taken(&self) -> u64 {
        self.taken.load(Ordering::Acquire)
    }
}

impl ManagementSocketProvider for ProtectedSocketFdSource {
    fn take_fd(&self) -> Result<i32, SocketSeamError> {
        let fd = self.queue.lock().expect("fd queue").pop_front();
        match fd {
            Some(fd) => {
                self.taken.fetch_add(1, Ordering::AcqRel);
                Ok(fd.into_raw_fd())
            }
            None => Err(SocketSeamError::NoSocket),
        }
    }

    fn take_for_dial(&self) -> Pin<Box<dyn Future<Output = Result<SocketLease, SocketSeamError>> + Send + '_>> {
        Box::pin(async move {
            let queued = self.queue.lock().expect("fd queue").pop_front();
            if let Some(fd) = queued {
                self.taken.fetch_add(1, Ordering::AcqRel);
                return Ok(SocketLease::Owned(fd));
            }
            let (supply, purpose) = self.supply.get().ok_or(SocketSeamError::NoSocket)?;
            let owned = supply.request(*purpose).await?;
            self.taken.fetch_add(1, Ordering::AcqRel);
            Ok(SocketLease::Owned(owned))
        })
    }
}

// ---------------------------------------------------------------------------
// fd helpers (tun.rs dup idiom, ledger-less: native-originated sockets)
// ---------------------------------------------------------------------------

/// Dup `raw` into an owned copy: `F_DUPFD_CLOEXEC` first, plain `dup()` +
/// `F_SETFD(FD_CLOEXEC)` fallback, F_GETFD probe before and after (the
/// [`crate::tun::TunFd::dup_from_raw`] idiom). The provided number is never
/// closed here.
pub fn dup_socket_fd(raw: i32) -> Result<i32, SocketSeamError> {
    let fd_dup = dup_fd_untraced(raw)?;
    crate::n2_socket_trace::duplicated(raw, fd_dup);
    Ok(fd_dup)
}

/// The same dup contract WITHOUT the N2 socket-trace mirror. One caller only:
/// [`crate::n2_stimulus`]'s outer-socket registry, which holds its OWN dup of
/// a socket that is already in the trace and must not add a second,
/// never-closed copy of that fd to the trace map.
pub fn dup_socket_fd_untraced(raw: i32) -> Result<i32, SocketSeamError> {
    dup_fd_untraced(raw)
}

fn dup_fd_untraced(raw: i32) -> Result<i32, SocketSeamError> {
    if unsafe { sys::fcntl(raw, sys::F_GETFD) } == -1 {
        return Err(SocketSeamError::BadFd { errno: sys::errno() });
    }
    let mut via_dupfd = true;
    let mut fd_dup = unsafe { sys::fcntl(raw, sys::F_DUPFD_CLOEXEC, 0) };
    if fd_dup == -1 {
        via_dupfd = false;
        fd_dup = unsafe { sys::dup(raw) };
    }
    if fd_dup == -1 {
        return Err(SocketSeamError::BadFd { errno: sys::errno() });
    }
    if unsafe { sys::fcntl(raw, sys::F_GETFD) } == -1 {
        let e = sys::errno();
        unsafe { sys::close(fd_dup) };
        return Err(SocketSeamError::BadFd { errno: e });
    }
    if !via_dupfd {
        unsafe { sys::fcntl(fd_dup, sys::F_SETFD, sys::FD_CLOEXEC) };
    }
    Ok(fd_dup)
}

/// Read-only probe used by the start/feed validation: is `fd` an open
/// descriptor, and is it dup-able? The probe dup is closed immediately.
pub fn probe_fd_dupable(fd: i32) -> Result<(), SocketSeamError> {
    match dup_socket_fd(fd) {
        Ok(dup) => {
            crate::n2_socket_trace::closed(dup);
            unsafe { sys::close(dup) };
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// `O_NONBLOCK` on the DUP copy. Side effect (documented in the module docs):
/// the provided number shares the open-file description, so its flag flips
/// too — provider sides must not do blocking I/O after hand-over.
fn set_nonblock(fd: i32) -> Result<(), SocketSeamError> {
    let fl = unsafe { sys::fcntl(fd, sys::F_GETFL) };
    if fl == -1 {
        return Err(SocketSeamError::NonBlock { errno: sys::errno() });
    }
    if unsafe { sys::fcntl(fd, sys::F_SETFL, fl | sys::O_NONBLOCK) } == -1 {
        return Err(SocketSeamError::NonBlock { errno: sys::errno() });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// the dial
// ---------------------------------------------------------------------------

/// Why a protected dial failed. Tokens/errnos only — never secrets, never
/// server text (module credential discipline).
#[derive(Debug)]
pub enum SocketDialError {
    Seam(SocketSeamError),
    /// connect(2) failed (errno; the dup copy is already closed).
    Connect { errno: i32 },
}

impl SocketDialError {
    pub fn token(&self) -> &'static str {
        match self {
            SocketDialError::Seam(e) => e.token(),
            SocketDialError::Connect { .. } => "socket-connect-failed",
        }
    }

    fn errno(&self) -> i32 {
        match self {
            SocketDialError::Seam(e) => e.errno(),
            SocketDialError::Connect { errno } => *errno,
        }
    }
}

impl core::fmt::Display for SocketDialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} (errno={})", self.token(), self.errno())
    }
}

impl std::error::Error for SocketDialError {}

/// One protected dial, from a freshly-taken socket to a connected
/// [`tokio::net::TcpStream`]:
///
/// 1. `source.take_for_dial()` obtains a lease on a socket the shell protected
///    BEFORE this dial (production queue owns a feed-time duplicate);
///    `Err` fails closed;
/// 2. dup (`F_DUPFD_CLOEXEC`) for Tokio connect while the lease stays live;
/// 3. `O_NONBLOCK` on the dup;
/// 4. `connect(2)` to `connect_addr` AFTER protect. An already connected
///    socket (`EISCONN`) is adopted via another dup of the live lease.
pub async fn connect_protected(
    source: &dyn ManagementSocketProvider,
    connect_addr: SocketAddr,
) -> Result<tokio::net::TcpStream, SocketDialError> {
    let lease = source.take_for_dial().await.map_err(SocketDialError::Seam)?;
    let provided = lease.fd();
    let dup = dup_socket_fd(provided).map_err(SocketDialError::Seam)?;
    if let Err(e) = set_nonblock(dup) {
        unsafe { sys::close(dup) };
        return Err(SocketDialError::Seam(e));
    }
    // TcpSocket owns the dup from here: on the error paths below it is
    // dropped (closing exactly the dup — the provided number is untouched).
    // SAFETY: `dup` is a fresh, owned descriptor from dup_socket_fd above.
    crate::n2_socket_trace::io_begin(dup, connect_addr);
    match unsafe { tokio::net::TcpSocket::from_raw_fd(dup) }.connect(connect_addr).await {
        Ok(stream) => {
            crate::n2_socket_trace::first_io(stream.as_raw_fd(), connect_addr, "connected", "connect");
            Ok(stream)
        },
        Err(e) if e.raw_os_error() == Some(sys::EISCONN) => {
            // Already-connected socket handed to us (host tests): adopt it
            // through a fresh dup of the still-open provided number.
            let dup2 = dup_socket_fd(provided).map_err(SocketDialError::Seam)?;
            if let Err(e) = set_nonblock(dup2) {
                unsafe { sys::close(dup2) };
                return Err(SocketDialError::Seam(e));
            }
            // from_std panics on a blocking fd; O_NONBLOCK is already set.
            // SAFETY: `dup2` is a fresh, owned descriptor from dup_socket_fd.
            let std_stream = unsafe { std::net::TcpStream::from_raw_fd(dup2) };
            tokio::net::TcpStream::from_std(std_stream).map(|stream| {
                crate::n2_socket_trace::first_io(stream.as_raw_fd(), connect_addr, "connected", "already_connected");
                stream
            }).map_err(|e| {
                SocketDialError::Connect { errno: e.raw_os_error().unwrap_or(0) }
            })
        }
        Err(e) => {
            crate::n2_socket_trace::first_io(dup, connect_addr, "failed", "connect");
            Err(SocketDialError::Connect { errno: e.raw_os_error().unwrap_or(0) })
        },
    }
}

/// tonic connector service: EVERY invocation (the initial dial AND every
/// re-dial tonic/hyper performs when the pooled connection dies) takes a
/// FRESH protected socket from the provider — per-dial re-protect, upstream
/// `ControlProtectSocket` semantics (`dialer_init_android.go:4-6`).
pub struct ProtectedSocketConnector {
    source: Arc<dyn ManagementSocketProvider>,
    connect_addr: SocketAddr,
}

impl ProtectedSocketConnector {
    /// Build the connector service over `source` (per-dial fresh sockets).
    pub fn new(source: Arc<dyn ManagementSocketProvider>, connect_addr: SocketAddr) -> Self {
        ProtectedSocketConnector { source, connect_addr }
    }
}

impl Service<Uri> for ProtectedSocketConnector {
    type Response = TokioIo<tokio::net::TcpStream>;
    type Error = SocketDialError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let source = self.source.clone();
        let addr = self.connect_addr;
        Box::pin(async move {
            connect_protected(source.as_ref(), addr).await.map(TokioIo::new)
        })
    }
}

// ---------------------------------------------------------------------------
// shell-side socket pre-open (the wg_fwd_open pattern, TCP variant)
// ---------------------------------------------------------------------------

/// `mgmt_socket_open() -> string` (JSON `{fd, bind_rc, bind_errno}`): open a
/// TCP socket (`SOCK_STREAM|SOCK_CLOEXEC`) and bind `0.0.0.0:0` WITHOUT
/// connecting, so the shell can `VpnConnection.protect(fd)` BEFORE any
/// packet flows (the verified split of the WG outer socket,
/// spikes/n1b-disc-phys-hap e8e2cf9 — governance item 4: protect before the
/// first connect AND before every reconnect). The connect itself happens on
/// a dup copy in [`connect_protected`], after the protect.
///
/// fd ownership: shell receives the opened original and closes it after a
/// successful feed or on failure. The feed synchronously duplicates the fd;
/// core owns only that duplicate.
pub fn mgmt_socket_open() -> String {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_STREAM | sys::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        let e = sys::errno();
        return format!("{{\"fd\":-1,\"bind_rc\":-1,\"bind_errno\":{e}}}");
    }
    let sa = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let bind_rc =
        unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) };
    let bind_errno = if bind_rc == -1 { sys::errno() } else { 0 };
    if bind_rc != 0 {
        unsafe { sys::close(fd) };
        return format!("{{\"fd\":-1,\"bind_rc\":{bind_rc},\"bind_errno\":{bind_errno}}}");
    }
    hilog::emit(&format!(
        "mgmtsock: tcp socket pre-opened for protect gate fd={fd} (unconnected, protected=nothing-yet)"
    ));
    format!("{{\"fd\":{fd},\"bind_rc\":0,\"bind_errno\":0}}")
}

/// Device-path ICE socket opener (N5c/N7 gap): same shape as
/// [`mgmt_socket_open`] but SOCK_DGRAM and deliberately **unbound** — the ICE
/// consumer (`ice::ProtectedUdpFdSource`) dups the fd, binds the COPY to the
/// chosen interface address and reads the chosen port back via `getsockname`
/// (see `ice.rs` module docs). Binding here would pick the port before the
/// interface is known.
///
/// fd ownership is identical to the TCP opener: shell closes the original
/// after `connector_ice_socket_feed` duplicates it or on feed failure.
pub fn udp_socket_open() -> String {
    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM | sys::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        let e = sys::errno();
        return format!("{{\"fd\":-1,\"bind_rc\":-1,\"bind_errno\":{e}}}");
    }
    hilog::emit(&format!(
        "mgmtsock: udp socket pre-opened for ICE protect gate fd={fd} (unbound, protected=nothing-yet)"
    ));
    format!("{{\"fd\":{fd},\"bind_rc\":0,\"bind_errno\":0}}")
}

// ---------------------------------------------------------------------------
// tests (host: Linux — same syscall numbers/constants as the OHOS target)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_stable_and_secret_free() {
        assert_eq!(SocketSeamError::NoSocket.token(), "no-protected-socket");
        assert_eq!(
            SocketSeamError::BadFd { errno: 9 }.token(),
            "socket-fd-invalid"
        );
        assert_eq!(
            SocketDialError::Connect { errno: 111 }.token(),
            "socket-connect-failed"
        );
        // display carries only token + errno
        let d = format!("{}", SocketDialError::Connect { errno: 111 });
        assert_eq!(d, "socket-connect-failed (errno=111)");
    }

    #[test]
    fn empty_source_fails_closed_with_no_socket() {
        let src = ProtectedSocketFdSource::new_with_fd(-1).expect("empty tcp source");
        assert_eq!(src.pending(), 0);
        let err = src.take_fd().unwrap_err();
        assert_eq!(err, SocketSeamError::NoSocket);
        assert_eq!(src.taken(), 0);
    }

    #[test]
    fn invalid_seed_fd_is_fallible() {
        assert!(ProtectedSocketFdSource::new_with_fd(i32::MAX).is_err());
    }

    #[test]
    fn source_is_fifo_and_counts_take() {
        let raw: Vec<i32> = (0..3).map(|_| unsafe {
            sys::socket(sys::AF_INET, sys::SOCK_STREAM | sys::SOCK_CLOEXEC, 0)
        }).collect();
        assert!(raw.iter().all(|fd| *fd >= 0));
        let src = ProtectedSocketFdSource::new_with_fd(raw[0]).unwrap();
        src.feed(raw[1]).unwrap();
        src.feed(raw[2]).unwrap();
        assert_eq!(src.pending(), 3);
        for fd in raw {
            unsafe { sys::close(fd) };
        }
        for _ in 0..3 {
            let owned = src.take_fd().unwrap();
            assert!(unsafe { sys::fcntl(owned, sys::F_GETFD) } >= 0);
            unsafe { sys::close(owned) };
        }
        assert_eq!(src.take_fd().unwrap_err(), SocketSeamError::NoSocket);
        assert_eq!(src.taken(), 3, "one take per handed-out socket");
        assert_eq!(src.pending(), 0);
    }

    #[test]
    fn management_and_signal_feeds_keep_original_socket_when_number_is_reused() {
        unsafe extern "C" { fn dup2(oldfd: i32, newfd: i32) -> i32; fn listen(fd: i32, backlog: i32) -> i32; }
        for _purpose in ["management", "signal"] {
            let original = unsafe { sys::socket(sys::AF_INET, sys::SOCK_STREAM, 0) };
            assert!(original >= 0);
            let source = ProtectedSocketFdSource::new_with_fd(-1).expect("empty tcp source");
            source.feed(original).unwrap();
            unsafe { sys::close(original) };
            let replacement = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(replacement >= 0);
            if replacement != original {
                assert_eq!(unsafe { dup2(replacement, original) }, original);
            }
            let fed = source.take_fd().unwrap();
            assert_ne!(fed, original);
            assert_eq!(unsafe { listen(fed, 1) }, 0, "queued TCP must not become replacement UDP");
            unsafe { sys::close(fed); sys::close(original); }
            if replacement != original { unsafe { sys::close(replacement) }; }
        }
    }

    #[test]
    fn clear_pending_discards_management_and_signal_copies() {
        let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_STREAM, 0) };
        assert!(fd >= 0);
        let source = ProtectedSocketFdSource::new_with_fd(-1).expect("empty tcp source");
        source.feed(fd).unwrap();
        unsafe { sys::close(fd) };
        assert_eq!(source.pending(), 1);
        source.clear_pending();
        assert_eq!(source.pending(), 0);
        assert_eq!(source.take_fd().unwrap_err(), SocketSeamError::NoSocket);
    }

    #[test]
    fn dup_is_distinct_and_original_survives() {
        // a REAL socket to dup: an unconnected, bound TCP socket
        let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_STREAM | sys::SOCK_CLOEXEC, 0) };
        assert!(fd >= 0);
        let sa = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        assert_eq!(
            unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
            0
        );
        let dup = dup_socket_fd(fd).expect("dup");
        assert_ne!(dup, fd, "dup must be a distinct descriptor number");
        // original still open and usable by its owner
        assert!(unsafe { sys::fcntl(fd, sys::F_GETFD) } != -1);
        // probe closes its own dup only
        probe_fd_dupable(fd).expect("probe");
        assert!(unsafe { sys::fcntl(fd, sys::F_GETFD) } != -1);
        // closing the dup never kills the original
        unsafe { sys::close(dup) };
        assert!(unsafe { sys::fcntl(fd, sys::F_GETFD) } != -1);
        unsafe { sys::close(fd) };

        // dead fd numbers are rejected (fail-closed validation path)
        assert_eq!(
            probe_fd_dupable(-1).unwrap_err(),
            SocketSeamError::BadFd { errno: sys::EBADF }
        );
        let e = dup_socket_fd(-1).unwrap_err();
        assert_eq!(e.token(), "socket-fd-invalid");
    }

    #[test]
    fn mgmt_socket_open_json_shape() {
        let json = mgmt_socket_open();
        let doc = crate::config::parse_document(&json).expect("valid JSON");
        match doc {
            crate::config::Json::Obj(entries) => {
                let fd = entries.iter().find(|(k, _)| k == "fd");
                let bind_rc = entries.iter().find(|(k, _)| k == "bind_rc");
                let bind_errno = entries.iter().find(|(k, _)| k == "bind_errno");
                let (fd, bind_rc) = match (fd, bind_rc, bind_errno) {
                    (
                        Some((_, crate::config::Json::Num(fd))),
                        Some((_, crate::config::Json::Num(rc))),
                        Some((_, crate::config::Json::Num(_))),
                    ) => (*fd as i32, *rc as i32),
                    other => panic!("unexpected shape: {other:?}"),
                };
                assert_eq!(bind_rc, 0, "loopback bind must succeed");
                // keep the fd bounded: close it again right away
                assert!(fd >= 0);
                assert!(unsafe { sys::fcntl(fd, sys::F_GETFD) } != -1);
                unsafe { sys::close(fd) };
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
