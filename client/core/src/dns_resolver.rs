//! Pure DNS snapshots and wire/configuration plans. No sockets, threads or cache.
//! Official semantics: NetBird 67c2041 engine.go, dns/server.go, host.go,
//! local/local.go. External CNAME lookup continues through the target domain's
//! resolution plan, which need not use physical DNS.
use crate::network_map::DnsConfig;
use hickory_proto::{
    op::{Edns, Message, MessageType, OpCode, Query, ResponseCode},
    rr::{DNSClass, Name, RData, Record, RecordType},
    serialize::txt::RDataParser,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsError(pub String);
impl std::fmt::Display for DnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DnsError {}
type Result<T> = std::result::Result<T, DnsError>;
fn err(e: impl std::fmt::Display) -> DnsError {
    DnsError(e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsDecision {
    Reply(Vec<u8>),
    Forward {
        /// Flattening of ONLY the selected domain's groups, for convenience.
        servers: Vec<SocketAddr>,
        /// Race groups; retain server order within each management group.
        server_groups: Vec<Vec<SocketAddr>>,
        allow_physical_fallback: bool,
    },
    /// Resolve query through the target domain's resolution plan, then
    /// complete_external_cname; do not force physical DNS.
    ExternalCname {
        query: Vec<u8>,
        response_prefix: Vec<u8>,
    },
}
#[derive(Debug, Clone)]
struct Zone {
    domain: Name,
    non_authoritative: bool,
}
#[derive(Debug, Clone)]
struct Upstream {
    domain: Name,
    groups: Vec<Vec<SocketAddr>>,
}
#[derive(Debug, Clone, Default)]
pub struct DnsResolverSnapshot {
    zones: Vec<Zone>,
    records: Vec<Record>,
    upstreams: Vec<Upstream>,
    search: Vec<String>,
    skipped_records: usize,
    skipped_zones: usize,
    skipped_nameservers: usize,
    skipped_group_domains: usize,
}
fn name(s: &str) -> Result<Name> {
    Name::from_utf8(s)
        .map(|n| {
            let mut n = n.to_lowercase();
            n.set_fqdn(true);
            n
        })
        .map_err(err)
}
fn wire(m: &Message) -> Result<Vec<u8>> {
    m.to_vec().map_err(err)
}
fn decode(b: &[u8]) -> Result<Message> {
    Message::from_vec(b).map_err(err)
}
fn add_search(out: &mut Vec<String>, n: &Name) {
    let s = n.to_utf8();
    if !out.contains(&s) {
        out.push(s);
    }
}
impl DnsResolverSnapshot {
    pub fn new(config: Option<&DnsConfig>, runtime_ip: Ipv4Addr) -> Result<Self> {
        let mut snapshot = Self::default();
        let Some(config) = config else {
            return Ok(snapshot);
        };
        // Permanent service applies records/groups even when management is disabled
        // (server.go:648-675). ServiceEnable remains management-owned state.
        for zone in &config.custom_zones {
            let domain = match name(&zone.domain) {
                Ok(domain) if !zone.domain.is_empty() => domain,
                _ => {
                    snapshot.skipped_zones += 1;
                    snapshot.skipped_records += zone.records.len();
                    continue;
                }
            };
            // Empty record zones are skipped only for local handler routing.
            if zone.records.is_empty() {
                continue;
            }
            snapshot.zones.push(Zone {
                domain,
                non_authoritative: zone.non_authoritative && config.custom_zones.len() != 1,
            });
            for r in &zone.records {
                // server.go:864-868 accepts only literal IN; local.go:642-646
                // skips each NewRR failure rather than rejecting the update.
                let parsed = (|| -> Result<Record> {
                    if r.class != "IN" {
                        return Err(err("unsupported record class"));
                    }
                    let kind = RecordType::from(u16::try_from(r.r#type).map_err(err)?);
                    let data = RData::try_from_str(kind, &r.r_data).map_err(err)?;
                    let mut record = Record::from_rdata(
                        name(&r.name)?, u32::try_from(r.ttl).map_err(err)?, data,
                    );
                    record.set_dns_class(DNSClass::IN);
                    Ok(record)
                })();
                match parsed {
                    Ok(record) => snapshot.records.push(record),
                    Err(_) => snapshot.skipped_records += 1,
                }
            }
        }
        for group in &config.name_server_groups {
            if group.name_servers.is_empty() {
                continue;
            }
            let mut valid_domains = Vec::new();
            for d in &group.domains {
                match name(d) {
                    Ok(n) if !d.is_empty() => valid_domains.push(n),
                    _ => snapshot.skipped_group_domains += 1,
                }
            }
            if group.search_domains_enabled {
                for d in &valid_domains {
                    add_search(&mut snapshot.search, d);
                }
            }
            let mut servers = Vec::new();
            for ns in &group.name_servers {
                if ns.ns_type != 1 {
                    snapshot.skipped_nameservers += 1;
                    continue;
                } // official UDPNameServerType
                let Ok(ip) = ns.ip.parse::<IpAddr>() else {
                    snapshot.skipped_nameservers += 1;
                    continue;
                };
                if ip == IpAddr::V4(runtime_ip) {
                    snapshot.skipped_nameservers += 1;
                    continue;
                }
                servers.push(SocketAddr::new(ip, ns.port));
            }
            let domains = if group.primary {
                vec![Name::root()]
            } else {
                valid_domains
            };
            for domain in domains {
                // server.go:934-946 skips unusable root handlers, allowing
                // physical fallback. Private domains deliberately fail closed.
                if domain.is_root() && servers.is_empty() {
                    continue;
                }
                if let Some(u) = snapshot.upstreams.iter_mut().find(|u| u.domain == domain) {
                    if !servers.is_empty() {
                        u.groups.push(servers.clone());
                    }
                } else {
                    snapshot.upstreams.push(Upstream {
                        domain,
                        groups: if servers.is_empty() {
                            vec![]
                        } else {
                            vec![servers.clone()]
                        },
                    });
                }
            }
        }
        // host.go includes even empty zones after the group domains.
        for zone in &config.custom_zones {
            if !zone.search_domain_disabled && !zone.domain.is_empty() {
                if let Ok(domain) = name(&zone.domain) {
                    add_search(&mut snapshot.search, &domain);
                }
            }
        }
        snapshot
            .zones
            .sort_by_key(|z| std::cmp::Reverse(z.domain.num_labels()));
        snapshot
            .upstreams
            .sort_by_key(|u| std::cmp::Reverse(u.domain.num_labels()));
        Ok(snapshot)
    }
    /// Rejected records, including all records belonging to invalid zones.
    /// Counts only; configuration names and record values are never logged.
    pub fn skipped_records(&self) -> usize {
        self.skipped_records
    }
    /// Invalid/empty zone names (valid zones with no records are simply inactive).
    pub fn skipped_zones(&self) -> usize {
        self.skipped_zones
    }
    /// Invalid IPs, unsupported transports and self-addresses.
    pub fn skipped_nameservers(&self) -> usize {
        self.skipped_nameservers
    }
    /// Invalid/empty domain elements, without discarding valid sibling domains.
    pub fn skipped_group_domains(&self) -> usize {
        self.skipped_group_domains
    }
    /// Lowercase deduplicated FQDN suffixes. Search flags do not affect routing.
    pub fn search_domains(&self) -> Vec<String> {
        self.search.clone()
    }
    fn matching_zone(&self, n: &Name) -> Option<&Zone> {
        self.zones.iter().find(|z| z.domain.zone_of(n))
    }
    fn exists(&self, n: &Name, kind: RecordType) -> bool {
        self.records.iter().any(|r| r.name() == n)
            || (kind != RecordType::NS
                && kind != RecordType::SOA
                && self
                    .wild(n)
                    .is_some_and(|w| self.records.iter().any(|r| r.name() == &w)))
    }
    fn wild(&self, n: &Name) -> Option<Name> {
        if n.is_root() {
            return None;
        }
        Name::from_utf8(format!("*.{}", n.base_name().to_utf8())).ok()
    }
    fn exact(&self, n: &Name, kind: RecordType, class: DNSClass) -> Vec<Record> {
        self.records
            .iter()
            .filter(|r| r.name() == n && r.record_type() == kind && r.dns_class() == class)
            .cloned()
            .collect()
    }
    fn lookup(&self, n: &Name, kind: RecordType, class: DNSClass, wildcard: bool) -> Vec<Record> {
        let direct = self.exact(n, kind, class);
        if !direct.is_empty() || !wildcard {
            return direct;
        }
        self.wild(n)
            .map(|w| {
                self.exact(&w, kind, class)
                    .into_iter()
                    .map(|mut r| {
                        r.set_name(n.clone());
                        r
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn forward(&self, n: &Name) -> DnsDecision {
        if let Some(u) = self.upstreams.iter().find(|u| u.domain.zone_of(n)) {
            DnsDecision::Forward {
                servers: u.groups.iter().flatten().copied().collect(),
                server_groups: u.groups.clone(),
                allow_physical_fallback: false,
            }
        } else {
            DnsDecision::Forward {
                servers: vec![],
                server_groups: vec![],
                allow_physical_fallback: true,
            }
        }
    }
    /// Finish an ExternalCname plan against this same immutable snapshot.
    /// Unlike the wire-only helper, also applies non-authoritative NXDOMAIN
    /// fallthrough to the original owner, as local.ServeDNS does.
    pub fn complete_external_cname(
        &self,
        response_prefix: &[u8],
        external_reply: &[u8],
    ) -> Result<DnsDecision> {
        let reply = complete_external_cname(response_prefix, external_reply)?;
        let response = decode(&reply)?;
        let owner = response.queries()[0].name().to_lowercase();
        if response.response_code() == ResponseCode::NXDomain
            && self
                .matching_zone(&owner)
                .is_some_and(|z| z.non_authoritative)
        {
            return Ok(self.forward(&owner));
        }
        Ok(DnsDecision::Reply(reply))
    }
    pub fn resolve(&self, query: &[u8]) -> Result<DnsDecision> {
        let request = decode(query)?;
        if request.message_type() != MessageType::Query || request.queries().len() != 1 {
            return Ok(DnsDecision::Reply(error_response(
                query,
                ResponseCode::FormErr,
            )?));
        }
        if request.op_code() != OpCode::Query {
            return Ok(DnsDecision::Reply(error_response(
                query,
                ResponseCode::NotImp,
            )?));
        }
        if request
            .extensions()
            .as_ref()
            .is_some_and(|e| e.version() != 0)
        {
            return Ok(DnsDecision::Reply(error_response(
                query,
                ResponseCode::BADVERS,
            )?));
        }
        let q = &request.queries()[0];
        let n = q.name().to_lowercase();
        let Some(zone) = self.matching_zone(&n) else {
            return Ok(self.forward(&n));
        };
        let kind = q.query_type();
        let mut response = response_base(&request, ResponseCode::NoError);
        response.set_authoritative(true);
        let wildcard = kind != RecordType::NS && kind != RecordType::SOA;
        let mut answers = self.lookup(
            &n,
            kind,
            q.query_class(),
            wildcard && !self.records.iter().any(|r| r.name() == &n),
        );
        let mut rcode = ResponseCode::NoError;
        if answers.is_empty() && kind != RecordType::CNAME {
            let mut target = n.clone();
            for _ in 0..8 {
                let aliases = self.lookup(&target, RecordType::CNAME, q.query_class(), wildcard);
                let Some(first) = aliases.first() else {
                    break;
                };
                let RData::CNAME(next) = first.data() else {
                    break;
                };
                target = next.0.to_lowercase();
                answers.extend(aliases);
                let terminal = self.exact(&target, kind, q.query_class());
                if !terminal.is_empty() {
                    answers.extend(terminal);
                    break;
                }
                if !self
                    .exact(&target, RecordType::CNAME, q.query_class())
                    .is_empty()
                {
                    continue;
                }
                if self.exists(&target, kind) {
                    break;
                }
                if self.matching_zone(&target).is_some() {
                    rcode = ResponseCode::NXDomain;
                    break;
                }
                if kind != RecordType::A && kind != RecordType::AAAA {
                    rcode = ResponseCode::NotImp;
                    break;
                }
                response.add_answers(answers);
                let mut external = Message::new();
                external
                    .set_id(request.id())
                    .set_recursion_desired(true)
                    .add_query(Query::query(target, kind));
                return Ok(DnsDecision::ExternalCname {
                    query: wire(&external)?,
                    response_prefix: wire(&response)?,
                });
            }
        }
        if rcode == ResponseCode::NoError && answers.is_empty() && !self.exists(&n, kind) {
            rcode = ResponseCode::NXDomain;
        }
        if rcode == ResponseCode::NXDomain && zone.non_authoritative {
            return Ok(self.forward(&n));
        }
        response.set_response_code(rcode).add_answers(answers);
        Ok(DnsDecision::Reply(wire(&response)?))
    }
}
fn response_base(query: &Message, code: ResponseCode) -> Message {
    let mut response = Message::new();
    response
        .set_id(query.id())
        .set_message_type(MessageType::Response)
        .set_op_code(query.op_code())
        .set_recursion_desired(query.recursion_desired())
        .set_checking_disabled(query.checking_disabled())
        .set_recursion_available(true)
        .set_response_code(code)
        .add_queries(query.queries().iter().cloned());
    if let Some(request_edns) = query.extensions() {
        let mut edns = Edns::new();
        edns.set_version(0)
            .set_max_payload(request_edns.max_payload().max(512))
            .set_dnssec_ok(request_edns.flags().dnssec_ok);
        response.set_edns(edns);
    }
    response
}
pub fn error_response(query: &[u8], rcode: ResponseCode) -> Result<Vec<u8>> {
    wire(&response_base(&decode(query)?, rcode))
}
/// Complete ExternalCname using the target domain's resolution-plan response. Preserve original
/// ID/question/CNAMEs, propagate rcode, clear AA, synthesize official TTL60.
pub fn complete_external_cname(response_prefix: &[u8], external_reply: &[u8]) -> Result<Vec<u8>> {
    let mut prefix = decode(response_prefix)?;
    let external = decode(external_reply)?;
    let kind = prefix
        .queries()
        .first()
        .ok_or_else(|| err("missing prefix question"))?
        .query_type();
    if kind != RecordType::A && kind != RecordType::AAAA {
        return Err(err("external CNAME supports A/AAAA only"));
    }
    let target = prefix
        .answers()
        .iter()
        .rev()
        .find_map(|r| match r.data() {
            RData::CNAME(c) => Some(c.0.to_lowercase()),
            _ => None,
        })
        .ok_or_else(|| err("missing CNAME prefix"))?;
    let q = external
        .queries()
        .first()
        .ok_or_else(|| err("missing external question"))?;
    if external.message_type() != MessageType::Response
        || external.id() != prefix.id()
        || external.queries().len() != 1
        || q.name().to_lowercase() != target
        || q.query_type() != kind
        || q.query_class() != DNSClass::IN
    {
        return Err(err("external CNAME response does not match target"));
    }
    prefix
        .set_authoritative(false)
        .set_response_code(external.response_code())
        .set_truncated(external.truncated());
    if external.response_code() == ResponseCode::NoError {
        for record in external
            .answers()
            .iter()
            .filter(|r| r.record_type() == kind && r.dns_class() == DNSClass::IN)
        {
            let mut record = record.clone();
            record.set_name(target.clone()).set_ttl(60);
            prefix.add_answer(record);
        }
    }
    wire(&prefix)
}
/// Official GetLastIPFromNetwork(prefix, 1): last address minus one, even /31,
/// /32. No fixed overlay prefix.
pub fn runtime_ip_from_cidr(cidr: &str) -> Result<Ipv4Addr> {
    let (ip, bits) = cidr
        .split_once('/')
        .ok_or_else(|| err("missing IPv4 prefix"))?;
    let ip: Ipv4Addr = ip.parse().map_err(err)?;
    let bits: u32 = bits.parse().map_err(err)?;
    if bits > 32 {
        return Err(err("IPv4 prefix exceeds 32"));
    }
    let host_mask = if bits == 0 {
        u32::MAX
    } else if bits == 32 {
        0
    } else {
        u32::MAX >> bits
    };
    let last = (u32::from(ip) | host_mask)
        .checked_sub(1)
        .ok_or_else(|| err("last address underflow"))?;
    // Go big.Int.Bytes -> AddrFromSlice rejects fewer than four bytes.
    if last < 0x0100_0000 {
        return Err(err("official IPv4 result has fewer than four bytes"));
    }
    Ok(last.into())
}
/// Limit by EDNS (512 without OPT) and caller budget (MTU minus IP/UDP headers).
/// Remove whole RRsets and re-encode; never cut bytes. Error if even the
/// question/OPT envelope cannot fit.
pub fn fit_udp_response(query: &[u8], reply: &[u8], max_payload: usize) -> Result<Vec<u8>> {
    let request = decode(query)?;
    let limit = max_payload.min(
        request
            .extensions()
            .as_ref()
            .map(|e| usize::from(e.max_payload()).max(512))
            .unwrap_or(512),
    );
    let mut response = decode(reply)?;
    loop {
        let bytes = wire(&response)?;
        if bytes.len() <= limit {
            return Ok(bytes);
        }
        response.set_truncated(true);
        let section = if !response.additionals().is_empty() {
            response.additionals_mut()
        } else if !response.name_servers().is_empty() {
            response.name_servers_mut()
        } else if !response.answers().is_empty() {
            response.answers_mut()
        } else {
            return Err(err("DNS question/EDNS envelope exceeds UDP payload budget"));
        };
        let last = section
            .last()
            .cloned()
            .ok_or_else(|| err("empty DNS section"))?;
        section.retain(|r| {
            !(r.name() == last.name()
                && r.record_type() == last.record_type()
                && r.dns_class() == last.dns_class())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_map::{CustomZone, NameServer, NameServerGroup, SimpleRecord};
    fn rec(n: &str, t: i64, data: &str) -> SimpleRecord {
        SimpleRecord {
            name: n.into(),
            r#type: t,
            class: "IN".into(),
            ttl: 90,
            r_data: data.into(),
        }
    }
    fn zone(d: &str, records: Vec<SimpleRecord>) -> CustomZone {
        CustomZone {
            domain: d.into(),
            records,
            search_domain_disabled: false,
            non_authoritative: false,
        }
    }
    fn config(zones: Vec<CustomZone>) -> DnsConfig {
        DnsConfig {
            service_enable: true,
            name_server_groups: vec![],
            custom_zones: zones,
        }
    }
    fn snap(c: &DnsConfig) -> DnsResolverSnapshot {
        DnsResolverSnapshot::new(Some(c), Ipv4Addr::new(100, 88, 255, 254)).unwrap()
    }
    fn query(n: &str, t: RecordType) -> Vec<u8> {
        let mut m = Message::new();
        m.set_id(0x8ace)
            .set_recursion_desired(true)
            .set_checking_disabled(true)
            .add_query(Query::query(Name::from_ascii(n).unwrap(), t));
        wire(&m).unwrap()
    }
    fn reply(s: &DnsResolverSnapshot, n: &str, t: RecordType) -> Message {
        match s.resolve(&query(n, t)).unwrap() {
            DnsDecision::Reply(b) => decode(&b).unwrap(),
            d => panic!("expected reply: {d:?}"),
        }
    }
    fn group(d: &[&str], primary: bool, ips: &[(&str, u16)]) -> NameServerGroup {
        NameServerGroup {
            name_servers: ips
                .iter()
                .map(|(ip, port)| NameServer {
                    ip: (*ip).into(),
                    port: *port,
                    ns_type: 1,
                })
                .collect(),
            primary,
            domains: d.iter().map(|s| (*s).into()).collect(),
            search_domains_enabled: false,
        }
    }
    fn forward(s: &DnsResolverSnapshot, n: &str) -> (Vec<SocketAddr>, Vec<Vec<SocketAddr>>, bool) {
        match s.resolve(&query(n, RecordType::A)).unwrap() {
            DnsDecision::Forward {
                servers,
                server_groups,
                allow_physical_fallback,
            } => (servers, server_groups, allow_physical_fallback),
            d => panic!("expected forward: {d:?}"),
        }
    }
    #[test]
    fn address_multirecord_and_original_question_flags() {
        let s = snap(&config(vec![zone(
            "netcenter.local",
            vec![
                rec("peer.netcenter.local", 1, "100.1.2.3"),
                rec("peer.netcenter.local", 1, "100.1.2.4"),
                rec("peer.netcenter.local", 28, "fd00::123"),
            ],
        )]));
        let m = reply(&s, "PEER.NetCenter.Local.", RecordType::A);
        assert_eq!(m.answers().len(), 2);
        assert_eq!(m.id(), 0x8ace);
        assert_eq!(m.queries()[0].name().to_utf8(), "PEER.NetCenter.Local.");
        assert!(
            m.authoritative()
                && m.recursion_available()
                && m.recursion_desired()
                && m.checking_disabled()
        );
        assert!(!m.authentic_data());
        assert_eq!(m.response_code(), ResponseCode::NoError);
        assert!(matches!(m.answers()[0].data(), RData::A(_)));
        assert!(matches!(
            reply(&s, "peer.netcenter.local", RecordType::AAAA).answers()[0].data(),
            RData::AAAA(_)
        ));
    }
    #[test]
    fn cname_chain_and_terminal_aaaa() {
        let s = snap(&config(vec![zone(
            "mesh.test",
            vec![
                rec("a.mesh.test", 5, "B.mesh.test."),
                rec("b.mesh.test", 5, "c.mesh.test."),
                rec("c.mesh.test", 1, "10.1.2.3"),
                rec("c.mesh.test", 28, "fd00::4"),
            ],
        )]));
        for t in [RecordType::A, RecordType::AAAA] {
            let m = reply(&s, "a.mesh.test", t);
            assert_eq!(m.answers().len(), 3);
            assert_eq!(m.answers()[2].record_type(), t);
            assert_eq!(m.response_code(), ResponseCode::NoError);
        }
        let m = reply(&s, "a.mesh.test", RecordType::CNAME);
        assert_eq!(m.answers().len(), 1);
        assert_eq!(m.answers()[0].record_type(), RecordType::CNAME);
    }
    #[test]
    fn nxdomain_nodata_and_cname_terminal_codes() {
        let s = snap(&config(vec![zone(
            "mesh.test",
            vec![
                rec("a.mesh.test", 1, "10.1.1.1"),
                rec("gone.mesh.test", 5, "missing.mesh.test."),
                rec("nodata.mesh.test", 5, "a.mesh.test."),
            ],
        )]));
        assert_eq!(
            reply(&s, "absent.mesh.test", RecordType::A).response_code(),
            ResponseCode::NXDomain
        );
        let nodata = reply(&s, "a.mesh.test", RecordType::AAAA);
        assert_eq!(nodata.response_code(), ResponseCode::NoError);
        assert!(nodata.answers().is_empty());
        let missing = reply(&s, "gone.mesh.test", RecordType::AAAA);
        assert_eq!(missing.response_code(), ResponseCode::NXDomain);
        assert_eq!(missing.answers().len(), 1);
        let nodata = reply(&s, "nodata.mesh.test", RecordType::AAAA);
        assert_eq!(nodata.response_code(), ResponseCode::NoError);
        assert_eq!(nodata.answers().len(), 1);
    }
    #[test]
    fn ptr_txt_srv_and_other_hickory_rdata() {
        let records = vec![
            rec("r.mesh.test", 12, "peer.mesh.test."),
            rec("r.mesh.test", 16, "\"hello world\" \"two\""),
            rec("r.mesh.test", 33, "10 20 8443 target.mesh.test."),
            rec("r.mesh.test", 15, "5 mail.mesh.test."),
        ];
        let s = snap(&config(vec![zone("mesh.test", records)]));
        for t in [
            RecordType::PTR,
            RecordType::TXT,
            RecordType::SRV,
            RecordType::MX,
        ] {
            let m = reply(&s, "r.mesh.test", t);
            assert_eq!(m.answers().len(), 1);
            assert_eq!(m.answers()[0].record_type(), t);
        }
        let m = reply(&s, "r.mesh.test", RecordType::TXT);
        match m.answers()[0].data() {
            RData::TXT(t) => assert_eq!(t.txt_data()[0].as_ref(), b"hello world"),
            _ => panic!(),
        }
        match reply(&s, "r.mesh.test", RecordType::SRV).answers()[0].data() {
            RData::SRV(s) => assert_eq!((s.priority(), s.weight(), s.port()), (10, 20, 8443)),
            _ => panic!(),
        }
    }
    #[test]
    fn single_zone_non_authoritative_compatibility() {
        let mut z = zone("mesh.test", vec![rec("a.mesh.test", 1, "10.1.1.1")]);
        z.non_authoritative = true;
        let s = snap(&config(vec![z]));
        assert_eq!(
            reply(&s, "missing.mesh.test", RecordType::A).response_code(),
            ResponseCode::NXDomain
        );
    }
    #[test]
    fn non_authoritative_only_nxdomain_falls_through() {
        let mut z = zone("mesh.test", vec![rec("a.mesh.test", 1, "10.1.1.1")]);
        z.non_authoritative = true;
        let mut c = config(vec![
            z,
            zone("other.test", vec![rec("a.other.test", 1, "10.2.2.2")]),
        ]);
        c.name_server_groups
            .push(group(&["mesh.test"], false, &[("10.10.0.1", 5300)]));
        let s = snap(&c);
        assert_eq!(
            forward(&s, "missing.mesh.test").0,
            vec!["10.10.0.1:5300".parse().unwrap()]
        );
        assert!(!forward(&s, "missing.mesh.test").2);
        assert!(reply(&s, "a.mesh.test", RecordType::AAAA)
            .answers()
            .is_empty());
        assert!(reply(&s, "a.mesh.test", RecordType::A).authoritative());
    }
    #[test]
    fn suffix_boundaries_and_local_priority() {
        let mut c = config(vec![zone(
            "netcenter.local",
            vec![rec("a.deep.netcenter.local", 1, "10.1.1.1")],
        )]);
        c.name_server_groups = vec![group(&["deep.netcenter.local"], false, &[("10.2.2.2", 53)])];
        let s = snap(&c);
        assert_eq!(
            reply(&s, "A.DEEP.NETCENTER.LOCAL.", RecordType::A)
                .answers()
                .len(),
            1
        );
        assert!(forward(&s, "evilnetcenter.local").2);
        assert!(forward(&s, "netcenter.local.evil").2);
        assert_eq!(
            reply(&s, "missing.deep.netcenter.local", RecordType::A).response_code(),
            ResponseCode::NXDomain
        );
    }
    #[test]
    fn most_specific_domain_and_primary_are_separate() {
        let mut c = config(vec![]);
        c.name_server_groups = vec![
            group(&[], true, &[("1.1.1.1", 53)]),
            group(&["mesh.test"], false, &[("10.0.0.1", 54)]),
            group(&["deep.mesh.test"], false, &[("10.0.0.2", 55)]),
        ];
        let s = snap(&c);
        assert_eq!(forward(&s, "a.deep.mesh.test").0[0].port(), 55);
        assert_eq!(forward(&s, "a.mesh.test").0[0].port(), 54);
        assert_eq!(
            forward(&s, "public.test").0[0].ip(),
            "1.1.1.1".parse::<IpAddr>().unwrap()
        );
        assert!(!forward(&s, "public.test").2);
        c.name_server_groups.remove(0);
        assert!(forward(&snap(&c), "public.test").2);
    }
    #[test]
    fn same_domain_groups_keep_boundaries_and_ports() {
        let mut c = config(vec![]);
        c.name_server_groups = vec![
            group(
                &["mesh.test"],
                false,
                &[("10.0.0.1", 5300), ("10.0.0.2", 5400)],
            ),
            group(&["MESH.TEST."], false, &[("2001:db8::1", 5500)]),
        ];
        let (flat, groups, fallback) = forward(&snap(&c), "a.mesh.test");
        assert!(!fallback);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[2].port(), 5500);
    }
    #[test]
    fn self_and_non_udp_filtered_without_private_leak() {
        let mut c = config(vec![]);
        let mut g = group(
            &["mesh.test"],
            false,
            &[("100.88.255.254", 5300), ("10.0.0.1", 5400)],
        );
        g.name_servers.push(NameServer {
            ip: "10.0.0.2".into(),
            ns_type: 0,
            port: 53,
        });
        c.name_server_groups.push(g);
        assert_eq!(
            forward(&snap(&c), "a.mesh.test").0,
            vec!["10.0.0.1:5400".parse().unwrap()]
        );
        c.name_server_groups[0].name_servers.remove(1);
        c.name_server_groups
            .push(group(&[], true, &[("1.1.1.1", 53)]));
        let (servers, _, fallback) = forward(&snap(&c), "a.mesh.test");
        assert!(servers.is_empty());
        assert!(!fallback);
    }
    #[test]
    fn empty_disabled_and_absent_config() {
        let mut c = config(vec![zone("mesh.test", vec![])]);
        c.name_server_groups.push(group(&[], false, &[]));
        assert!(forward(&snap(&c), "x.mesh.test").2);
        c.custom_zones[0]
            .records
            .push(rec("x.mesh.test", 1, "10.0.0.1"));
        c.service_enable = false;
        c.name_server_groups.push(group(&["private.test"], false, &[("10.0.0.2", 53)]));
        let s = snap(&c);
        assert_eq!(reply(&s, "x.mesh.test", RecordType::A).answers().len(), 1);
        assert!(!forward(&s, "x.private.test").2);
        assert_eq!(s.search_domains(), vec!["mesh.test."]);
        assert!(
            forward(
                &DnsResolverSnapshot::new(None, Ipv4Addr::LOCALHOST).unwrap(),
                "x.mesh.test"
            )
            .2
        );
    }
    #[test]
    fn replacing_snapshot_deletes_records_and_groups() {
        let mut c = config(vec![zone(
            "mesh.test",
            vec![rec("x.mesh.test", 1, "10.0.0.1")],
        )]);
        c.service_enable = false;
        c.name_server_groups.push(group(&["private.test"], false, &[("10.0.0.2", 53)]));
        let old = snap(&c);
        assert_eq!(reply(&old, "x.mesh.test", RecordType::A).answers().len(), 1);
        assert!(!forward(&old, "a.private.test").2);
        c.custom_zones.clear();
        c.name_server_groups.clear();
        let new = snap(&c);
        assert!(forward(&new, "x.mesh.test").2);
        assert!(forward(&new, "a.private.test").2);
        assert!(new.search_domains().is_empty());
        let absent = DnsResolverSnapshot::new(None, Ipv4Addr::LOCALHOST).unwrap();
        assert!(forward(&absent, "x.mesh.test").2);
        assert!(forward(&absent, "a.private.test").2);
        assert_eq!(reply(&old, "x.mesh.test", RecordType::A).answers().len(), 1);
    }
    #[test]
    fn search_flags_are_independent_from_routing() {
        let mut a = zone("one.test", vec![rec("a.one.test", 1, "10.0.0.1")]);
        a.search_domain_disabled = true;
        let mut c = config(vec![a, zone("TWO.TEST", vec![])]);
        let mut g = group(&["THREE.TEST"], false, &[("10.0.0.2", 53)]);
        g.search_domains_enabled = true;
        c.name_server_groups = vec![g, group(&["four.test"], false, &[("10.0.0.3", 53)])];
        let s = snap(&c);
        let search = s.search_domains();
        assert!(search.contains(&"two.test.".into()));
        assert!(search.contains(&"three.test.".into()));
        assert_eq!(search.len(), 2);
        assert_eq!(reply(&s, "a.one.test", RecordType::A).answers().len(), 1);
        assert!(!forward(&s, "a.four.test").2);
    }
    #[test]
    fn udp_truncation_is_decodable_whole_rrsets_and_tc() {
        let records = (0..12)
            .map(|_| rec("big.mesh.test", 16, &format!("\"{}\"", "x".repeat(150))))
            .collect();
        let s = snap(&config(vec![zone("mesh.test", records)]));
        let q = query("big.mesh.test", RecordType::TXT);
        let DnsDecision::Reply(raw) = s.resolve(&q).unwrap() else {
            panic!()
        };
        assert!(raw.len() > 1500);
        let fitted = fit_udp_response(&q, &raw, 1472).unwrap();
        let m = decode(&fitted).unwrap();
        assert!(fitted.len() <= 512);
        assert!(m.truncated());
        assert!(m.answers().is_empty());
        let mut request = decode(&q).unwrap();
        let mut edns = Edns::new();
        edns.set_max_payload(4096);
        request.set_edns(edns);
        let q = wire(&request).unwrap();
        let DnsDecision::Reply(raw) = s.resolve(&q).unwrap() else {
            panic!()
        };
        let fitted = fit_udp_response(&q, &raw, 1232).unwrap();
        assert!(fitted.len() <= 1232);
        assert!(decode(&fitted).unwrap().truncated());
        assert!(fit_udp_response(&q, &raw, 12).is_err());
    }
    #[test]
    fn edns_flags_version_and_error_response() {
        let s = snap(&config(vec![zone(
            "mesh.test",
            vec![rec("a.mesh.test", 1, "10.0.0.1")],
        )]));
        let mut m = decode(&query("a.mesh.test", RecordType::A)).unwrap();
        let mut edns = Edns::new();
        edns.set_max_payload(1232).set_dnssec_ok(true);
        m.set_edns(edns);
        let DnsDecision::Reply(b) = s.resolve(&wire(&m).unwrap()).unwrap() else {
            panic!()
        };
        let r = decode(&b).unwrap();
        assert_eq!(r.extensions().as_ref().unwrap().max_payload(), 1232);
        assert!(r.extensions().as_ref().unwrap().flags().dnssec_ok);
        m.extensions_mut().as_mut().unwrap().set_version(1);
        let DnsDecision::Reply(b) = s.resolve(&wire(&m).unwrap()).unwrap() else {
            panic!()
        };
        assert_eq!(
            u16::from(decode(&b).unwrap().response_code()),
            u16::from(ResponseCode::BADVERS)
        );
        let q = query("a.mesh.test", RecordType::A);
        let e = decode(&error_response(&q, ResponseCode::ServFail).unwrap()).unwrap();
        assert_eq!(e.id(), 0x8ace);
        assert_eq!(e.response_code(), ResponseCode::ServFail);
        assert!(!e.authoritative());
    }
    #[test]
    fn malformed_requests_and_query_class() {
        let s = snap(&config(vec![zone(
            "mesh.test",
            vec![rec("a.mesh.test", 1, "10.0.0.1")],
        )]));
        assert!(s.resolve(&[0, 1]).is_err());
        let q = wire(&Message::new()).unwrap();
        let DnsDecision::Reply(b) = s.resolve(&q).unwrap() else {
            panic!()
        };
        assert_eq!(decode(&b).unwrap().response_code(), ResponseCode::FormErr);
        let mut q = decode(&query("a.mesh.test", RecordType::A)).unwrap();
        q.queries_mut()[0].set_query_class(DNSClass::CH);
        let DnsDecision::Reply(b) = s.resolve(&wire(&q).unwrap()).unwrap() else {
            panic!()
        };
        let r = decode(&b).unwrap();
        assert!(r.answers().is_empty());
        assert_eq!(r.queries()[0].query_class(), DNSClass::CH);
        q.set_op_code(OpCode::Update);
        let DnsDecision::Reply(b) = s.resolve(&wire(&q).unwrap()).unwrap() else {
            panic!()
        };
        assert_eq!(decode(&b).unwrap().response_code(), ResponseCode::NotImp);
    }
    #[test]
    fn invalid_items_do_not_reject_valid_configuration() {
        let mut bad_class = rec("a.netcenter.local", 1, "10.0.0.1");
        bad_class.class = "CH".into();
        let mut negative_ttl = rec("a.netcenter.local", 1, "10.0.0.1");
        negative_ttl.ttl = -1;
        let mut large_ttl = negative_ttl.clone();
        large_ttl.ttl = i64::from(u32::MAX) + 1;
        let mut zero_ttl = rec("zero.netcenter.local", 1, "10.0.0.3");
        zero_ttl.ttl = 0;
        let mut c = config(vec![
            zone("netcenter.local", vec![
                rec("bad.netcenter.local", 1, "no-ip"),
                rec("bad.netcenter.local", 28, "no-ipv6"),
                rec("bad.netcenter.local", 65000, "unknown"),
                rec("bad.netcenter.local", -1, "10.0.0.1"),
                rec("bad.netcenter.local", 65536, "10.0.0.1"),
                bad_class, negative_ttl, large_ttl, zero_ttl,
                rec("peer.netcenter.local", 1, "100.1.2.3"),
            ]),
            zone("other.test", vec![rec("peer.other.test", 28, "fd00::1")]),
            zone("", vec![rec("lost.test", 1, "10.0.0.1")]),
            zone("bad..test", vec![rec("lost.test", 1, "10.0.0.1")]),
        ]);
        c.name_server_groups = vec![
            group(&[], true, &[("invalid-ip", 53), ("1.1.1.1", 53)]),
            group(&["", "bad..test", "private.test"], false,
                &[("invalid-ip", 53), ("10.0.0.2", 5300)]),
            group(&[], false, &[("10.0.0.4", 53)]),
        ];
        c.name_server_groups[1].search_domains_enabled = true;
        let s = snap(&c);
        assert_eq!(s.skipped_records(), 10);
        assert_eq!(s.skipped_zones(), 2);
        assert_eq!(s.skipped_nameservers(), 2);
        assert_eq!(s.skipped_group_domains(), 2);
        assert_eq!(reply(&s, "peer.netcenter.local", RecordType::A).answers().len(), 1);
        assert_eq!(reply(&s, "peer.other.test", RecordType::AAAA).answers().len(), 1);
        assert_eq!(reply(&s, "zero.netcenter.local", RecordType::A).answers()[0].ttl(), 0);
        assert_eq!(forward(&s, "public.test").0, vec!["1.1.1.1:53".parse().unwrap()]);
        assert_eq!(forward(&s, "a.private.test").0, vec!["10.0.0.2:5300".parse().unwrap()]);
        assert_eq!(s.search_domains(), vec!["private.test.", "netcenter.local.", "other.test."]);
    }
    #[test]
    fn filtered_root_allows_fallback_but_private_remains_closed() {
        for self_address in [false, true] {
            let mut g = group(&[], true, &[(if self_address { "100.88.255.254" } else { "10.0.0.1" }, 53)]);
            if !self_address { g.name_servers[0].ns_type = 0; }
            let mut c = config(vec![]);
            c.name_server_groups.push(g.clone());
            assert!(forward(&snap(&c), "public.test").2);
            g.primary = false;
            g.domains = vec!["private.test".into()];
            c.name_server_groups.push(g);
            let s = snap(&c);
            let (servers, _, fallback) = forward(&s, "a.private.test");
            assert!(servers.is_empty());
            assert!(!fallback);
            assert!(forward(&s, "public.test").2);
        }
    }
    #[test]
    fn wildcard_nodata_and_one_label_semantics() {
        let s = snap(&config(vec![zone(
            "mesh.test",
            vec![
                rec("*.mesh.test", 1, "10.0.0.1"),
                rec("existing.mesh.test", 28, "fd00::1"),
            ],
        )]));
        let r = reply(&s, "fresh.mesh.test", RecordType::A);
        assert_eq!(r.answers().len(), 1);
        assert_eq!(r.answers()[0].name(), &name("fresh.mesh.test").unwrap());
        assert!(reply(&s, "existing.mesh.test", RecordType::A)
            .answers()
            .is_empty());
        assert_eq!(
            reply(&s, "deep.fresh.mesh.test", RecordType::A).response_code(),
            ResponseCode::NXDomain
        );
    }
    #[test]
    fn external_cname_explicit_continuation_and_codes() {
        let s = snap(&config(vec![zone(
            "mesh.test",
            vec![rec("a.mesh.test", 5, "external.example.")],
        )]));
        let DnsDecision::ExternalCname {
            query: q,
            response_prefix: p,
        } = s.resolve(&query("a.mesh.test", RecordType::A)).unwrap()
        else {
            panic!()
        };
        let target = decode(&q).unwrap();
        assert_eq!(
            target.queries()[0].name(),
            &name("external.example").unwrap()
        );
        let mut external = response_base(&target, ResponseCode::NoError);
        external.add_answer(Record::from_rdata(
            name("external.example").unwrap(),
            1234,
            RData::A("192.0.2.1".parse().unwrap()),
        ));
        let r = decode(&complete_external_cname(&p, &wire(&external).unwrap()).unwrap()).unwrap();
        assert_eq!(r.answers().len(), 2);
        assert_eq!(r.answers()[1].ttl(), 60);
        assert!(!r.authoritative());
        assert_eq!(r.queries()[0].name(), &name("a.mesh.test").unwrap());
        let r = decode(
            &complete_external_cname(&p, &error_response(&q, ResponseCode::NXDomain).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(r.response_code(), ResponseCode::NXDomain);
        assert_eq!(r.answers().len(), 1);
        external.set_id(7);
        assert!(complete_external_cname(&p, &wire(&external).unwrap()).is_err());
        assert_eq!(
            reply(&s, "a.mesh.test", RecordType::TXT).response_code(),
            ResponseCode::NotImp
        );
    }
    #[test]
    fn external_cname_nxdomain_non_authoritative_fallthrough() {
        let mut z = zone(
            "mesh.test",
            vec![rec("a.mesh.test", 5, "external.example.")],
        );
        z.non_authoritative = true;
        let mut c = config(vec![
            z,
            zone("other.test", vec![rec("a.other.test", 1, "10.0.0.1")]),
        ]);
        c.name_server_groups
            .push(group(&["mesh.test"], false, &[("10.0.0.2", 5300)]));
        let s = snap(&c);
        let DnsDecision::ExternalCname {
            query: q,
            response_prefix: p,
        } = s.resolve(&query("a.mesh.test", RecordType::A)).unwrap()
        else {
            panic!()
        };
        let result = s
            .complete_external_cname(&p, &error_response(&q, ResponseCode::NXDomain).unwrap())
            .unwrap();
        assert!(matches!(
            result,
            DnsDecision::Forward {
                allow_physical_fallback: false,
                ..
            }
        ));
        let result = s
            .complete_external_cname(&p, &error_response(&q, ResponseCode::ServFail).unwrap())
            .unwrap();
        let DnsDecision::Reply(b) = result else {
            panic!()
        };
        assert_eq!(decode(&b).unwrap().response_code(), ResponseCode::ServFail);
    }
    #[test]
    fn cname_loop_is_bounded_at_official_eight_hops() {
        let s = snap(&config(vec![zone(
            "mesh.test",
            vec![
                rec("a.mesh.test", 5, "b.mesh.test."),
                rec("b.mesh.test", 5, "a.mesh.test."),
            ],
        )]));
        let r = reply(&s, "a.mesh.test", RecordType::A);
        assert_eq!(r.answers().len(), 8);
        assert_eq!(r.response_code(), ResponseCode::NoError);
    }
    #[test]
    fn runtime_address_uses_actual_prefix_and_official_edge_behavior() {
        for (cidr, expected) in [
            ("100.88.12.3/16", "100.88.255.254"),
            ("172.20.1.5/24", "172.20.1.254"),
            ("192.0.2.5/31", "192.0.2.4"),
            ("192.0.2.5/32", "192.0.2.4"),
            ("0.0.0.0/0", "255.255.255.254"),
        ] {
            assert_eq!(
                runtime_ip_from_cidr(cidr).unwrap(),
                expected.parse::<Ipv4Addr>().unwrap()
            );
        }
        for bad in ["x", "::1/64", "192.0.2.1/33", "0.0.0.0/32", "0.0.0.5/32"] {
            assert!(runtime_ip_from_cidr(bad).is_err());
        }
    }
}
