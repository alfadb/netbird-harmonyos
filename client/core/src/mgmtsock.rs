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
//! The fd number crossing this seam stays OWNED BY THE PROVIDER SIDE
//! (production: the shell boundary that opened it via [`mgmt_socket_open`]
//! and protected it via `VpnConnection.protect`). Consumers here dup it
//! (`F_DUPFD_CLOEXEC`, `dup()`+`F_SETFD` fallback — the [`crate::tun`]
//! idiom) and NEVER read, write, ioctl or close the provided number: the
//! original remains usable/closeable by its owner, and its only sanctioned
//! closer is the platform side (`VpnConnection.destroy()` for the platform
//! TUN fd; these native-opened sockets are retired by their provider —
//! retirement policy is an N4+ increment, see the notes doc).
//!
//! One documented side effect: the dup shares the open-file description, so
//! the `O_NONBLOCK` set on the dup below flips the flag for the provided
//! number too. Provider sides must not do blocking I/O on a socket after
//! handing it over (governance item 2 lists this exact OFD question as a
//! registered obligation).
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
use std::os::fd::FromRawFd;
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
}

impl SocketSeamError {
    /// Stable error token (the only text that may cross the boundary).
    pub fn token(&self) -> &'static str {
        match self {
            SocketSeamError::NoSocket => "no-protected-socket",
            SocketSeamError::BadFd { .. } => "socket-fd-invalid",
            SocketSeamError::NonBlock { .. } => "socket-nonblock-failed",
        }
    }

    pub fn errno(&self) -> i32 {
        match self {
            SocketSeamError::NoSocket => 0,
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
}

/// Production provider: a FIFO queue of protected sockets seeded by
/// `connector_start_with_socket` (first socket) and refilled by the shell
/// via `connector_socket_feed` (fresh open+protect per socket). Empty queue
/// → `Err(NoSocket)` — fail-closed; reconnect dials keep failing until the
/// shell feeds a fresh protected socket (see the notes doc for the policy).
#[derive(Debug)]
pub struct ProtectedSocketFdSource {
    queue: Mutex<VecDeque<i32>>,
    taken: AtomicU64,
}

impl ProtectedSocketFdSource {
    /// Seed with the first protected socket.
    pub fn new_with_fd(fd: i32) -> Self {
        let mut q = VecDeque::with_capacity(2);
        if fd >= 0 {
            q.push_back(fd);
        }
        ProtectedSocketFdSource { queue: Mutex::new(q), taken: AtomicU64::new(0) }
    }

    /// Shell-side resupply: push another fresh protected socket fd.
    pub fn feed(&self, fd: i32) {
        if fd >= 0 {
            self.queue.lock().expect("fd queue").push_back(fd);
        }
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
                Ok(fd)
            }
            None => Err(SocketSeamError::NoSocket),
        }
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
/// 1. `source.take_fd()` — a socket the shell protected BEFORE this dial
///    (upstream `Dialer.Control` ordering, protectsocket_android.go:22-46);
///    `Err` fails the dial (fail-closed, no unprotected fallback);
/// 2. dup (`F_DUPFD_CLOEXEC`) — the ONLY use of the provided number; the
///    original is never read nor closed (fd contract);
/// 3. `O_NONBLOCK` on the dup;
/// 4. `connect(2)` to `connect_addr` — AFTER protect, on the dup. An already
///    connected provided socket (`EISCONN`: host-test pre-connected sockets /
///    socketpairs) is adopted via a second dup instead.
pub async fn connect_protected(
    source: &dyn ManagementSocketProvider,
    connect_addr: SocketAddr,
) -> Result<tokio::net::TcpStream, SocketDialError> {
    let provided = source.take_fd().map_err(SocketDialError::Seam)?;
    let dup = dup_socket_fd(provided).map_err(SocketDialError::Seam)?;
    if let Err(e) = set_nonblock(dup) {
        unsafe { sys::close(dup) };
        return Err(SocketDialError::Seam(e));
    }
    // TcpSocket owns the dup from here: on the error paths below it is
    // dropped (closing exactly the dup — the provided number is untouched).
    // SAFETY: `dup` is a fresh, owned descriptor from dup_socket_fd above.
    match unsafe { tokio::net::TcpSocket::from_raw_fd(dup) }.connect(connect_addr).await {
        Ok(stream) => Ok(stream),
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
            tokio::net::TcpStream::from_std(std_stream).map_err(|e| {
                SocketDialError::Connect { errno: e.raw_os_error().unwrap_or(0) }
            })
        }
        Err(e) => Err(SocketDialError::Connect {
            errno: e.raw_os_error().unwrap_or(0),
        }),
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
/// fd ownership: native-created, native-owned until handed back through
/// `connector_start_with_socket` / `connector_socket_feed`; from that point
/// the seam treats the number as provider-owned (dup-only, never closed —
/// fd contract in the module docs).
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
        let src = ProtectedSocketFdSource::new_with_fd(-1);
        assert_eq!(src.pending(), 0);
        let err = src.take_fd().unwrap_err();
        assert_eq!(err, SocketSeamError::NoSocket);
        assert_eq!(src.taken(), 0);
    }

    #[test]
    fn source_is_fifo_and_counts_take() {
        let src = ProtectedSocketFdSource::new_with_fd(21);
        src.feed(22);
        src.feed(23);
        assert_eq!(src.pending(), 3);
        assert_eq!(src.take_fd().unwrap(), 21);
        assert_eq!(src.take_fd().unwrap(), 22);
        assert_eq!(src.take_fd().unwrap(), 23);
        assert_eq!(src.take_fd().unwrap_err(), SocketSeamError::NoSocket);
        assert_eq!(src.taken(), 3, "one take per handed-out socket");
        assert_eq!(src.pending(), 0);
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
