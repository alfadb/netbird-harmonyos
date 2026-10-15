//! Independent DNS worker. Connector owns DnsRuntime; WG keeps only Arc<DnsHandle>.
//! Call stop_and_join outside WG locks. This module never accepts a TUN fd.
use crate::{
    dns_resolver::{error_response, fit_udp_response, DnsDecision, DnsResolverSnapshot},
    dns_transport::{DnsProtocol, DnsRequest, DnsTransport},
    dns_upstream::DnsUpstream,
    socket_supply::SocketSupply,
};
use hickory_proto::op::ResponseCode;
use smoltcp::wire::{IpProtocol, Ipv4Packet, TcpPacket, UdpPacket};
use std::io::{Read, Write};
use std::{
    collections::VecDeque,
    io,
    net::{Ipv4Addr, SocketAddr},
    os::{
        fd::{AsRawFd, RawFd},
        unix::net::UnixStream,
    },
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, Notify},
    task::JoinSet,
};
const QUEUE_LIMIT: usize = 128;
const MAX_QUERIES: usize = 64;
#[derive(Debug)]
pub struct DnsOutput {
    pub generation: u64,
    pub frame: Vec<u8>,
}
/// Domain-free counters; no query names or wire data are logged.
#[derive(Debug, Clone, Copy, Default)]
pub struct DnsStats {
    pub accepted: u64,
    pub dropped_input: u64,
    pub dropped_output: u64,
    pub queries: u64,
    pub failures: u64,
    pub running: bool,
}
struct Input {
    epoch: u64,
    frame: Vec<u8>,
}
struct Queues {
    output: VecDeque<DnsOutput>,
    snapshot: Arc<DnsResolverSnapshot>,
    generation: u64,
    reset: u64,
}
/// Nonblocking data-plane facade. Keep an Arc alive throughout poll(wake_fd).
/// Ingress capacity is independent of control and output queue contention.
pub struct DnsHandle {
    runtime_ip: Ipv4Addr,
    mtu: usize,
    ingress: mpsc::Sender<Input>,
    generation: AtomicU64,
    epoch: AtomicU64,
    queues: Mutex<Queues>,
    notify: Notify,
    wake_read: UnixStream,
    wake_write: UnixStream,
    running: AtomicBool,
    accepted: AtomicU64,
    dropped_input: AtomicU64,
    dropped_output: AtomicU64,
    queries: AtomicU64,
    failures: AtomicU64,
}
impl DnsHandle {
    /// Reliable queue-only TUN reset; never joins the runtime worker.
    pub fn reset_tun(&self, generation: u64) {
        let mut q = self.queues.lock().unwrap();
        q.generation = generation;
        q.reset = q.reset.wrapping_add(1);
        q.output.clear();
        self.generation.store(generation, Ordering::Release);
        self.epoch.store(q.reset, Ordering::Release);
        drop(q);
        self.notify.notify_one();
        self.signal();
    }

    pub fn is_dns_packet(&self, frame: &[u8]) -> bool {
        let Ok(ip) = Ipv4Packet::new_checked(frame) else {
            return false;
        };
        if ip.version() != 4
            || ip.dst_addr() != self.runtime_ip
            || ip.frag_offset() != 0
            || ip.more_frags()
        {
            return false;
        }
        match ip.next_header() {
            IpProtocol::Udp => {
                UdpPacket::new_checked(ip.payload()).is_ok_and(|p| p.dst_port() == 53)
            }
            IpProtocol::Tcp => {
                TcpPacket::new_checked(ip.payload()).is_ok_and(|p| p.dst_port() == 53)
            }
            _ => false,
        }
    }
    /// True means consumed DNS, including overload, stale generations and stop.
    pub fn try_ingest(&self, frame: &[u8], generation: u64) -> bool {
        if !self.is_dns_packet(frame) {
            return false;
        }
        // Read epoch first: observing a published reset also observes its generation.
        // A reset racing this send makes the old epoch stale at the worker.
        let epoch = self.epoch.load(Ordering::Acquire);
        if self.running.load(Ordering::Acquire)
            && frame.len() <= self.mtu
            && self.generation.load(Ordering::Acquire) == generation
            && self
                .ingress
                .try_send(Input {
                    epoch,
                    frame: frame.to_vec(),
                })
                .is_ok()
        {
            self.accepted.fetch_add(1, Ordering::Relaxed);
            self.notify.notify_one();
            return true;
        }
        self.dropped_input.fetch_add(1, Ordering::Relaxed);
        true
    }
    pub fn take_packets(&self, limit: usize) -> Vec<DnsOutput> {
        let Ok(mut q) = self.queues.try_lock() else {
            self.signal();
            return vec![];
        };
        let n = limit.min(q.output.len());
        let packets = q.output.drain(..n).collect();
        if !q.output.is_empty() {
            self.signal();
        }
        packets
    }
    pub fn wake_fd(&self) -> RawFd {
        self.wake_read.as_raw_fd()
    }
    fn signal(&self) {
        let byte = [1u8];
        // Both ends are owned by the handle; full means already readable.
        loop {
            match (&self.wake_write).write(&byte) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                _ => break,
            }
        }
    }
    /// Bounded even with continuous producers. Rearms remaining output.
    pub fn drain_wake(&self) {
        let mut bytes = [0u8; 256];
        for _ in 0..16 {
            if !matches!((&self.wake_read).read(&mut bytes), Ok(n) if n > 0) {
                break;
            }
        }
        match self.queues.try_lock() {
            Ok(q) if q.output.is_empty() => (),
            _ => self.signal(),
        }
    }
    pub fn stats(&self) -> DnsStats {
        DnsStats {
            accepted: self.accepted.load(Ordering::Relaxed),
            dropped_input: self.dropped_input.load(Ordering::Relaxed),
            dropped_output: self.dropped_output.load(Ordering::Relaxed),
            queries: self.queries.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            running: self.running.load(Ordering::Acquire),
        }
    }
}
/// Owner lives outside WG device locks. Drop joins as a leak-prevention backstop.
pub struct DnsRuntime {
    handle: Arc<DnsHandle>,
    worker: Option<JoinHandle<()>>,
}
impl DnsRuntime {
    pub fn start(
        runtime_ip: Ipv4Addr,
        mtu: usize,
        snapshot: Arc<DnsResolverSnapshot>,
        supply: Arc<SocketSupply>,
        generation: u64,
    ) -> io::Result<Self> {
        let transport = DnsTransport::new(runtime_ip, mtu, generation)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (wake_read, wake_write) = UnixStream::pair()?;
        wake_read.set_nonblocking(true)?;
        wake_write.set_nonblocking(true)?;
        let (ingress, input) = mpsc::channel(QUEUE_LIMIT);
        let handle = Arc::new(DnsHandle {
            runtime_ip,
            mtu,
            ingress,
            generation: AtomicU64::new(generation),
            epoch: AtomicU64::new(0),
            queues: Mutex::new(Queues {
                output: VecDeque::new(),
                snapshot,
                generation,
                reset: 0,
            }),
            notify: Notify::new(),
            wake_read,
            wake_write,
            running: AtomicBool::new(true),
            accepted: AtomicU64::new(0),
            dropped_input: AtomicU64::new(0),
            dropped_output: AtomicU64::new(0),
            queries: AtomicU64::new(0),
            failures: AtomicU64::new(0),
        });
        let shared = handle.clone();
        let worker = std::thread::Builder::new()
            .name("dns-runtime".into())
            .spawn(move || {
                runtime.block_on(worker(shared.clone(), transport, supply, input));
                shared.running.store(false, Ordering::Release);
                shared.signal();
            })?;
        Ok(Self {
            handle,
            worker: Some(worker),
        })
    }
    pub fn handle(&self) -> Arc<DnsHandle> {
        self.handle.clone()
    }
    /// New requests use this snapshot; in-flight tasks retain their old Arc.
    pub fn update(&self, snapshot: Arc<DnsResolverSnapshot>) {
        self.handle.queues.lock().unwrap().snapshot = snapshot;
        self.handle.notify.notify_one();
    }
    /// Reliable control, independent of data queue capacity. Also resets when the
    /// generation number is reused. Queued output is cleared immediately.
    pub fn reset_tun(&self, generation: u64) {
        self.handle.reset_tun(generation);
    }
    pub fn stop_and_join(&mut self) {
        self.handle.running.store(false, Ordering::Release);
        {
            let mut q = self.handle.queues.lock().unwrap();
            q.output.clear();
        }
        self.handle.notify.notify_one();
        self.handle.signal();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for DnsRuntime {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}
async fn worker(
    handle: Arc<DnsHandle>,
    mut transport: DnsTransport,
    supply: Arc<SocketSupply>,
    mut ingress: mpsc::Receiver<Input>,
) {
    let start = Instant::now();
    let now = || start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut tasks = JoinSet::new();
    let mut epoch = 0;
    loop {
        // Drain before sampling control so a newly reset input cannot be mistaken
        // for stale data against an earlier control snapshot. Work stays bounded.
        let input: Vec<_> = (0..QUEUE_LIMIT)
            .map_while(|_| ingress.try_recv().ok())
            .collect();
        // No configuration/control lock survives resolution or an await.
        let (snapshot, generation, reset) = {
            let q = handle.queues.lock().unwrap();
            (q.snapshot.clone(), q.generation, q.reset)
        };
        if !handle.running.load(Ordering::Acquire) {
            break;
        }
        if epoch != reset {
            tasks.abort_all();
            tasks = JoinSet::new();
            transport.reset();
            epoch = reset;
        }
        for input in input {
            if input.epoch == epoch {
                transport.ingest(&input.frame, now());
            } else {
                handle.dropped_input.fetch_add(1, Ordering::Relaxed);
            }
        }
        transport.poll(now(), 64);
        for request in transport.take_requests(QUEUE_LIMIT) {
            handle.queries.fetch_add(1, Ordering::Relaxed);
            let reply = match snapshot.resolve(&request.wire) {
                Ok(DnsDecision::Reply(wire)) => {
                    if valid_local_cnames(&wire) { Some(wire) } else { None }
                },
                Ok(plan) if tasks.len() < MAX_QUERIES => {
                    let snapshot = snapshot.clone();
                    let supply = supply.clone();
                    let ip = handle.runtime_ip;
                    tasks.spawn(async move {
                        let reply = execute(
                            snapshot,
                            supply,
                            ip,
                            &request.wire,
                            request.protocol == DnsProtocol::Tcp,
                            plan,
                        )
                        .await;
                        (epoch, request, reply)
                    });
                    continue;
                }
                _ => None,
            };
            respond(&handle, &mut transport, request, reply);
        }
        transport.poll(now(), 64);
        let packets = transport.take_packets(QUEUE_LIMIT);
        let output_ready = {
            let mut q = handle.queues.lock().unwrap();
            if q.reset == epoch && handle.running.load(Ordering::Acquire) {
                for frame in packets {
                    if q.output.len() < QUEUE_LIMIT {
                        q.output.push_back(DnsOutput { generation, frame });
                    } else {
                        handle.dropped_output.fetch_add(1, Ordering::Relaxed);
                    }
                }
                !q.output.is_empty()
            } else {
                false
            }
        };
        if output_ready {
            handle.signal();
        }
        let delay = transport
            .next_poll_ms(now())
            .map(|deadline| Duration::from_millis(deadline.saturating_sub(now())));
        tokio::select! {
            _ = handle.notify.notified() => {},
            result = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Ok((e, request, reply))) = result {
                    let current = handle.queues.lock().unwrap().reset;
                    if e == current && handle.running.load(Ordering::Acquire) { respond(&handle, &mut transport, request, reply); }
                }
            },
            _ = async { match delay { Some(d) => tokio::time::sleep(d).await, None => std::future::pending::<()>().await } } => {},
        }
    }
    ingress.close();
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    transport.reset();
}
fn respond(
    handle: &DnsHandle,
    transport: &mut DnsTransport,
    request: DnsRequest,
    reply: Option<Vec<u8>>,
) {
    let reply = reply.or_else(|| {
        handle.failures.fetch_add(1, Ordering::Relaxed);
        error_response(&request.wire, ResponseCode::ServFail).ok()
    });
    let Some(reply) = reply else { return };
    let reply = if request.protocol == DnsProtocol::Udp {
        let limit = hickory_proto::op::Message::from_vec(&request.wire)
            .ok()
            .map(|q| {
                q.extensions()
                    .as_ref()
                    .map(|e| usize::from(e.max_payload()).max(512))
                    .unwrap_or(512)
                    .min(handle.mtu.saturating_sub(28))
            });
        if limit.is_some_and(|n| reply.len() <= n) {
            // Keep opaque classified answers byte-for-byte when no fitting is needed.
            Some(reply)
        } else {
            fit_udp_response(&request.wire, &reply, handle.mtu.saturating_sub(28))
                .ok()
                .or_else(|| {
                    handle.failures.fetch_add(1, Ordering::Relaxed);
                    let failure = error_response(&request.wire, ResponseCode::ServFail).ok()?;
                    fit_udp_response(&request.wire, &failure, handle.mtu.saturating_sub(28)).ok()
                })
        }
    } else {
        Some(reply)
    };
    if let Some(reply) = reply {
        let _ = transport.respond(request.id, reply);
    }
}
fn valid_local_cnames(wire: &[u8]) -> bool {
    use hickory_proto::{op::Message, rr::RData};
    let Ok(reply) = Message::from_vec(wire) else { return false };
    let Some(question) = reply.queries().first() else { return true };
    // Explicit CNAME queries return records without following the chain.
    if question.query_type() == hickory_proto::rr::RecordType::CNAME {
        return true;
    }
    let mut owners = std::collections::HashSet::new();
    let mut hops = 0;
    for record in reply.answers() {
        if let RData::CNAME(target) = record.data() {
            hops += 1;
            if hops > 8 || !owners.insert(record.name().to_lowercase())
                || owners.contains(&target.0.to_lowercase()) {
                return false;
            }
        }
    }
    hops < 8 || reply.answers().iter().any(|r| r.record_type() == question.query_type())
}
async fn execute(
    snapshot: Arc<DnsResolverSnapshot>,
    supply: Arc<SocketSupply>,
    runtime_ip: Ipv4Addr,
    original: &[u8],
    tcp: bool,
    mut plan: DnsDecision,
) -> Option<Vec<u8>> {
    // Retain each question for non-authoritative fallthrough. Targets use the
    // same snapshot rather than OS getaddrinfo or an unconditional physical hop.
    let mut query = original.to_vec();
    let mut prefixes = Vec::new();
    let mut aliases = std::collections::HashSet::new();
    let mut hops = 0usize;
    loop {
        let reply = match plan {
            DnsDecision::Reply(wire) => {
                if !valid_local_cnames(&wire) { return None; }
                wire
            },
            DnsDecision::Forward {
                servers,
                mut server_groups,
                allow_physical_fallback,
            } => {
                if server_groups.is_empty() && !servers.is_empty() {
                    server_groups.push(servers);
                }
                let wire = match race_groups(supply.clone(), server_groups, &query, tcp).await {
                    Some(wire) => wire,
                    None if allow_physical_fallback => {
                        physical(supply.clone(), runtime_ip, &query, tcp).await?
                    }
                    None => return None,
                };
                if !prefixes.is_empty() {
                    use hickory_proto::{op::{Message, Query}, rr::RData};
                    let response = Message::from_vec(&wire).ok()?;
                    let question = response.queries().first()?;
                    let cnames: Vec<_> = response.answers().iter().filter_map(|r| {
                        if let RData::CNAME(target) = r.data() { Some((r, target)) } else { None }
                    }).collect();
                    if response.response_code() == ResponseCode::NoError && !cnames.is_empty() {
                        if !response.answers().iter().any(|r| r.record_type() == question.query_type()) {
                            let mut target = Message::new();
                            target.set_id(response.id()).set_recursion_desired(true).add_query(
                                Query::query(cnames.last()?.1.0.clone(), question.query_type()));
                            plan = DnsDecision::ExternalCname {
                                query: target.to_vec().ok()?, response_prefix: wire,
                            };
                            continue;
                        }
                        for (record, target) in cnames {
                            hops += 1;
                            if hops > 8 || !aliases.insert(record.name().to_lowercase())
                                || aliases.contains(&target.0.to_lowercase()) {
                                return None;
                            }
                        }
                    }
                }
                wire
            }
            DnsDecision::ExternalCname {
                query: target,
                response_prefix,
            } => {
                let prefix = hickory_proto::op::Message::from_vec(&response_prefix).ok()?;
                for record in prefix.answers() {
                    if record.record_type() == hickory_proto::rr::RecordType::CNAME {
                        hops += 1;
                        if hops > 8 || !aliases.insert(record.name().to_lowercase()) {
                            return None;
                        }
                    }
                }
                let target_message = hickory_proto::op::Message::from_vec(&target).ok()?;
                if aliases.contains(&target_message.queries().first()?.name().to_lowercase()) {
                    return None;
                }
                prefixes.push((query, response_prefix));
                query = target;
                plan = snapshot.resolve(&query).ok()?;
                continue;
            }
        };
        let Some((owner_query, prefix)) = prefixes.pop() else {
            return Some(reply);
        };
        query = owner_query;
        plan = snapshot.complete_external_cname(&prefix, &reply).ok()?;
        // Completion owns NXDOMAIN fallthrough and the original envelope. Its
        // address synthesis omits target-side aliases; retain those aliases and
        // their terminal owner when this continuation crossed another chain.
        if let DnsDecision::Reply(ref mut wire) = plan {
            use hickory_proto::{op::Message, rr::RecordType};
            let target = Message::from_vec(&reply).ok()?;
            let mut completed = Message::from_vec(wire).ok()?;
            let intermediate: Vec<_> = target.answers().iter()
                .filter(|r| r.record_type() == RecordType::CNAME).cloned().collect();
            if !intermediate.is_empty() {
                let mut answers = completed.take_answers();
                let mut terminal = answers.split_off(Message::from_vec(&prefix).ok()?.answers().len());
                let terminal_name = target.answers().iter().rev().find_map(|r| {
                    if let hickory_proto::rr::RData::CNAME(name) = r.data() {
                        Some(name.0.clone())
                    } else { None }
                })?;
                for record in &mut terminal { record.set_name(terminal_name.clone()); }
                answers.extend(intermediate);
                answers.extend(terminal);
                completed.add_answers(answers);
                *wire = completed.to_vec().ok()?;
            }
        }
    }
}
async fn physical(
    supply: Arc<SocketSupply>,
    runtime_ip: Ipv4Addr,
    query: &[u8],
    tcp: bool,
) -> Option<Vec<u8>> {
    let servers = supply
        .dns_servers()
        .into_iter()
        .filter(|ip| *ip != runtime_ip)
        .map(|ip| SocketAddr::from((ip, 53)))
        .collect();
    race_groups(supply, vec![servers], query, tcp).await
}
async fn race_groups(
    supply: Arc<SocketSupply>,
    groups: Vec<Vec<SocketAddr>>,
    query: &[u8],
    tcp: bool,
) -> Option<Vec<u8>> {
    let mut tasks = JoinSet::new();
    for group in groups {
        let query = query.to_vec();
        let supply = supply.clone();
        tasks.spawn(async move {
            let upstream = DnsUpstream::new(supply);
            for server in group {
                if let Ok(reply) = upstream.exchange(server, &query, tcp).await {
                    // Official forward retries SERVFAIL/REFUSED, accepts NXDOMAIN.
                    if !matches!(reply.wire.get(3).map(|b| b & 15), Some(2 | 5)) {
                        return Some(reply.wire);
                    }
                }
            }
            None
        });
    }
    while let Some(result) = tasks.join_next().await {
        if let Ok(Some(wire)) = result {
            return Some(wire);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Vec<u8> {
        use hickory_proto::{
            op::{Message, Query},
            rr::{Name, RecordType},
        };
        let mut query = Message::new();
        query.set_id(7).add_query(Query::query(
            Name::from_ascii("host.test").unwrap(),
            RecordType::A,
        ));
        let wire = query.to_vec().unwrap();
        let mut frame = vec![0; 28 + wire.len()];
        let len = frame.len();
        let mut ip = Ipv4Packet::new_unchecked(&mut frame[..]);
        ip.set_version(4);
        ip.set_header_len(20);
        ip.set_total_len(len as u16);
        ip.set_hop_limit(64);
        ip.set_next_header(IpProtocol::Udp);
        ip.set_src_addr(Ipv4Addr::new(100, 64, 0, 2));
        ip.set_dst_addr(Ipv4Addr::new(100, 100, 0, 53));
        ip.fill_checksum();
        let mut udp = UdpPacket::new_unchecked(&mut frame[20..]);
        udp.set_src_port(41000);
        udp.set_dst_port(53);
        udp.set_len((8 + wire.len()) as u16);
        udp.payload_mut().copy_from_slice(&wire);
        // IPv4 UDP permits an omitted checksum.
        frame
    }

    #[test]
    fn full_and_closed_ingress_consume_and_count_drops() {
        let ip = Ipv4Addr::new(100, 100, 0, 53);
        let snapshot = Arc::new(DnsResolverSnapshot::new(None, ip).unwrap());
        let mut runtime =
            DnsRuntime::start(ip, 1280, snapshot, Arc::new(SocketSupply::new()), 1).unwrap();
        let handle = runtime.handle();
        let frame = request();
        let guard = handle.queues.lock().unwrap();
        // The worker may already have drained one batch before blocking on
        // control. It cannot drain another batch while this guard is held.
        for _ in 0..QUEUE_LIMIT * 2 {
            if handle.ingress.capacity() == 0 {
                break;
            }
            assert!(handle.try_ingest(&frame, 1));
        }
        assert_eq!(handle.ingress.capacity(), 0);
        let before = handle.stats();
        assert!(handle.try_ingest(&frame, 1));
        let after = handle.stats();
        drop(guard);
        assert_eq!(after.accepted, before.accepted);
        assert_eq!(after.dropped_input, before.dropped_input + 1);
        // Full ingress cannot prevent reliable control or shutdown.
        handle.reset_tun(1);
        runtime.stop_and_join();
        assert!(handle.ingress.is_closed());
        let before = handle.stats();
        assert!(handle.try_ingest(&frame, 1));
        assert_eq!(handle.stats().accepted, before.accepted);
        assert_eq!(handle.stats().dropped_input, before.dropped_input + 1);
    }

    #[test]
    fn reset_discards_delayed_data_even_when_generation_is_reused() {
        let ip = Ipv4Addr::new(100, 100, 0, 53);
        let snapshot = Arc::new(DnsResolverSnapshot::new(None, ip).unwrap());
        let mut runtime =
            DnsRuntime::start(ip, 1280, snapshot, Arc::new(SocketSupply::new()), 1).unwrap();
        let handle = runtime.handle();
        // Model a producer paused after reading the epoch, before try_send.
        let old = Input {
            epoch: handle.epoch.load(Ordering::Acquire),
            frame: request(),
        };
        handle.reset_tun(1);
        assert!(handle.ingress.try_send(old).is_ok());
        assert!(handle.try_ingest(&request(), 1));
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if !handle.take_packets(1).is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "new epoch request was not answered"
            );
            std::thread::yield_now();
        }
        assert_eq!(handle.stats().queries, 1);
        assert_eq!(handle.stats().dropped_input, 1);
        runtime.stop_and_join();
    }

    #[test]
    fn ingress_accepts_while_control_output_is_locked() {
        let ip = Ipv4Addr::new(100, 100, 0, 53);
        let snapshot = Arc::new(DnsResolverSnapshot::new(None, ip).unwrap());
        let mut runtime =
            DnsRuntime::start(ip, 1280, snapshot, Arc::new(SocketSupply::new()), 1).unwrap();
        let handle = runtime.handle();
        let guard = handle.queues.lock().unwrap();
        let input_handle = handle.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let sender = std::thread::spawn(move || {
            let consumed = input_handle.try_ingest(&request(), 1);
            tx.send((consumed, input_handle.stats())).unwrap();
        });
        let result = rx.recv_timeout(Duration::from_millis(250));
        // Always release the lock before asserting so failure can still join.
        drop(guard);
        sender.join().unwrap();
        let (consumed, stats) = result.expect("ingress waited for the control/output lock");
        assert!(consumed);
        assert_eq!(stats.accepted, 1);
        assert_eq!(stats.dropped_input, 0);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(packet) = handle.take_packets(1).pop() {
                let ip = Ipv4Packet::new_checked(&packet.frame[..]).unwrap();
                let udp = UdpPacket::new_checked(ip.payload()).unwrap();
                let reply = hickory_proto::op::Message::from_vec(udp.payload()).unwrap();
                assert_eq!(packet.generation, 1);
                assert_eq!(reply.id(), 7);
                assert_eq!(
                    reply.message_type(),
                    hickory_proto::op::MessageType::Response
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "accepted request was not answered"
            );
            std::thread::yield_now();
        }
        assert_eq!(handle.stats().queries, 1);
        runtime.stop_and_join();
    }
}
