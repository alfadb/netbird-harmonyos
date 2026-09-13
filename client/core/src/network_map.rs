// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # network_map — minimal NetworkMap model (N3-4)
//!
//! Strict conversion of the management `NetworkMap` / `NetbirdConfig`
//! protobufs into the small model the HarmonyOS client actually needs for
//! its data plane. This is NOT a wholesale copy of the proto: every field
//! below is here because the upstream client reads it on the Sync path
//! (evidence: upstream `client/internal/engine.go` `updateNetworkMap`,
//! lines relative to pinned commit `791401060d2b95e5f51e3439c0649729132f571e`;
//! see `docs/n3-management-protocol-notes.md` §N3-4 for the full field→line
//! mapping). Fields the upstream client consumes that we do NOT consume yet
//! are listed in the module tail and in the doc — they are dropped at
//! conversion, never silently re-shaped.
//!
//! ## Model decisions
//!
//! - **Reuse, not duplication**: parsed IPv4 prefixes are
//!   [`crate::config::Route`] values — the exact type `config.rs` already
//!   uses for tunnel allowed-ips, so a future engine can feed management
//!   routes and peer allowed-ips straight into the WG configuration path.
//! - **Upstream masking semantics**: upstream converts `Route.network` with
//!   `netip.ParsePrefix` + `prefix.Masked()` (engine.go:1750-1755), i.e. a
//!   network with host bits set is accepted and MASKED, not rejected. We do
//!   the same for route networks; peer allowed-ips are canonicalized the
//!   same way (a WireGuard AllowedIPs entry with host bits set is malformed
//!   on the wire anyway). Anything that is not an IPv4 prefix (garbage or
//!   IPv6) is SKIPPED for routes and recorded in
//!   [`NetworkMap::skipped_routes`] — upstream logs and skips bad route
//!   networks (engine.go:1751-1753) and one bad route must not discard the
//!   whole snapshot. Peer allowed-ips, by contrast, are strict: a peer whose
//!   allowed-ips cannot parse is a hard error (wrong tunnel configuration).
//! - **Strictness elsewhere**: a remote peer without `wgPubKey`, an
//!   out-of-range name-server port, or a components-format envelope are
//!   explicit errors. Unknown proto FIELDS are tolerated (prost skips
//!   unknown tags; pinned by a test that decodes a NetworkMap carrying a
//!   future field 99).

use crate::config;
use crate::grpc::proto;
use crate::management::ManagementError;

// ---------------------------------------------------------------------------
// model
// ---------------------------------------------------------------------------

/// One management `NetworkMap` snapshot, reduced to what the client consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkMap {
    /// `NetworkMap.Serial` (field 1) — snapshot ordering. Upstream discards
    /// snapshots with a serial lower than the last applied one
    /// (engine.go:1572-1576); the consumer of this model does the same
    /// comparison, this struct only carries the value.
    pub serial: u64,
    /// Our own peer config (`NetworkMap.PeerConfig`, field 2).
    pub peer: Option<PeerSelfConfig>,
    /// Remote peers we may connect to (`remotePeers`, field 3).
    pub peers: Vec<PeerInfo>,
    /// `remotePeersIsEmpty` (field 4): distinguishes "management says the
    /// peer list is EMPTY" from "no peer list in this update" — the empty
    /// flag triggers a full peer teardown upstream (engine.go:1689-1696).
    pub peers_is_empty: bool,
    /// Peers currently offline (`offlinePeers`, field 7).
    pub offline_peers: Vec<PeerInfo>,
    /// Routes to apply (`Routes`, field 5). Entries whose network could not
    /// parse are skipped (upstream parity) and reported in [`NetworkMap::skipped_routes`].
    pub routes: Vec<ManagedRoute>,
    /// Raw strings of skipped route networks (see above) — skip is upstream
    /// behavior, but the loss is visible to the caller, never swallowed.
    pub skipped_routes: Vec<String>,
    /// DNS config (`DNSConfig`, field 6); `None` = absent from the snapshot.
    pub dns: Option<DnsConfig>,
}

/// Our own peer config (`PeerConfig` message; consumed upstream at
/// engine.go:1565→1329 updateConfig and toDNSFeatureFlag L1732-1736).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerSelfConfig {
    /// VPN IP of THIS peer (`PeerConfig.address`, field 1), e.g. "10.64.0.7".
    /// An address change forces a tunnel restart upstream (engine.go:1338-1343).
    pub address: Option<String>,
    /// Management DNS resolver address for the interface (`PeerConfig.dns`, field 2).
    pub interface_dns: Option<String>,
    /// Peer FQDN (`PeerConfig.fqdn`, field 4).
    pub fqdn: Option<String>,
    /// Explicit tunnel MTU (`PeerConfig.mtu`, field 7); `None` = field unset/0.
    pub mtu: Option<i32>,
    /// `RoutingPeerDnsResolutionEnabled` (field 5) — DNS feature flag
    /// (engine.go:1630, toDNSFeatureFlag L1732-1736).
    pub routing_peer_dns_resolution_enabled: bool,
    /// `LazyConnectionEnabled` (field 6) — lazy connection flag
    /// (engine.go:1580-1582).
    pub lazy_connection_enabled: bool,
}

/// A remote peer we are allowed to connect to (`RemotePeerConfig`).
///
/// NOTE (dispatch-assumption correction): `RemotePeerConfig` carries NO
/// relay and NO endpoint. Relay server URLs arrive once per sync in
/// `NetbirdConfig.relay` ([`NetbirdServers::relay`], consumed upstream at
/// engine.go:1144 handleRelayUpdate), and peer endpoints are negotiated via
/// ICE/signal, not management. The per-peer identity is `wg_pub_key` — there
/// is no numeric peer id in this message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// WireGuard public key, base64 string (`wgPubKey`, field 1). Upstream
    /// compares it as a string (own-peer filter engine.go:1681-1686, peer
    /// key peer/conn.go).
    pub wg_pub_key: String,
    /// Allowed IPs / routes towards this peer (`allowedIps`, field 2),
    /// parsed and masked into `config::Route` (host bits zeroed).
    pub allowed_ips: Vec<config::Route>,
    /// Peer FQDN (`fqdn`, field 4); empty on the wire → `None`.
    pub fqdn: Option<String>,
}

/// One management route (`Route` message → upstream `toRoutes`
/// engine.go:1739-1767).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRoute {
    /// Route id (`ID`, field 1).
    pub id: String,
    /// Routed network (`Network`, field 2), parsed to a masked
    /// `config::Route` prefix.
    pub network: config::Route,
    /// DNS domains this route serves (`Domains`, field 8, punycode as sent).
    pub domains: Vec<String>,
    /// Network id (`NetID`, field 7).
    pub net_id: String,
    /// Network type (`NetworkType`, field 3) — passed through; the routing
    /// policy (IPv4/IPv6/dual) is decided by a later increment.
    pub network_type: i64,
    /// Distribution/relay peer key (`Peer`, field 4).
    pub peer: String,
    /// Route metric (`Metric`, field 5).
    pub metric: i64,
    /// Masquerade flag (`Masquerade`, field 6).
    pub masquerade: bool,
    /// KeepRoute flag (`keepRoute`, field 9).
    pub keep_route: bool,
    /// SkipAutoApply flag (`skipAutoApply`, field 10).
    pub skip_auto_apply: bool,
}

/// DNS config (`DNSConfig` → upstream `toDNSConfig` engine.go:1792-1874).
/// Custom zones and the (deprecated) forwarder port are not consumed yet —
/// see the unconsumed-fields list in the module tail / doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsConfig {
    /// `ServiceEnable` (field 1) — whether the peer DNS resolver is active.
    pub service_enable: bool,
    /// Name server groups (field 2).
    pub name_server_groups: Vec<NameServerGroup>,
}

/// One name server group (`NameServerGroup`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameServerGroup {
    /// Servers, in management order (upstream preserves order and uses it
    /// as priority, toDNSConfig engine.go:1845-1857).
    pub name_servers: Vec<NameServer>,
    /// `Primary` (field 2).
    pub primary: bool,
    /// Match domains (field 3).
    pub domains: Vec<String>,
    /// `SearchDomainsEnabled` (field 4).
    pub search_domains_enabled: bool,
}

/// One resolver (`NameServer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameServer {
    /// Server IP as sent (`IP`, field 1; kept as string — may be IPv6).
    pub ip: String,
    /// UDP/TCP kind (`NSType`, field 2).
    pub ns_type: i64,
    /// Port (`Port`, field 3) — 0..=65535 enforced at conversion.
    pub port: u16,
}

/// Connection servers from `SyncResponse.netbirdConfig` (`NetbirdConfig`
/// message; consumed upstream in updateNetbirdConfig engine.go:1128-1186).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetbirdServers {
    /// STUN URIs (`stuns[].uri`).
    pub stuns: Vec<String>,
    /// TURN candidates (`turns[]`).
    pub turns: Vec<TurnServer>,
    /// Signal server URI (`signal.uri`); upstream notes "todo update signal"
    /// (engine.go:1185) — the value is consumed from the login response
    /// path today; kept here so the model is complete for the sync path too.
    pub signal: Option<String>,
    /// Relay servers (`relay`, consumed upstream at engine.go:1144).
    pub relay: Option<RelayServers>,
}

/// One TURN server (`ProtectedHostConfig`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnServer {
    /// `hostConfig.uri`.
    pub uri: String,
    /// `user`.
    pub user: String,
    /// `password`.
    pub password: String,
}

/// Relay server set (`RelayConfig`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayServers {
    /// Relay URLs (`urls`, field 1).
    pub urls: Vec<String>,
    /// Auth token payload (`tokenPayload`, field 2).
    pub token_payload: String,
    /// Auth token signature (`tokenSignature`, field 3).
    pub token_signature: String,
}

// ---------------------------------------------------------------------------
// conversion
// ---------------------------------------------------------------------------

/// Parse a management prefix string into a masked `config::Route`
/// (upstream `netip.ParsePrefix` + `prefix.Masked()`, engine.go:1750-1755).
/// IPv4 only — an IPv6 or garbage prefix is an explicit parse error
/// (the caller decides skip-vs-fail: routes skip upstream, peer allowed-ips
/// do not).
fn parse_masked_prefix_v4(s: &str) -> Result<config::Route, ManagementError> {
    let (addr_part, prefix_len) = match s.split_once('/') {
        None => (s, 32u8),
        Some((a, p)) => {
            let p: u8 = p.parse().map_err(|_| {
                ManagementError::Parse(format!("bad prefix length in route network {s:?}"))
            })?;
            if p > 32 {
                return Err(ManagementError::Parse(format!(
                    "prefix length {p} > 32 in route network {s:?}"
                )));
            }
            (a, p)
        }
    };
    // config::parse_ipv4 rejects anything with ':' (IPv6) or non-dotted-quad
    let addr = config::parse_ipv4(addr_part).ok_or_else(|| {
        ManagementError::Parse(format!(
            "route network {s:?} is not an IPv4 prefix (IPv6 routes not supported yet)"
        ))
    })?;
    // mask host bits like upstream prefix.Masked()
    let mask: u32 = if prefix_len == 0 { 0 } else { u32::MAX << (32 - prefix_len as u32) };
    let net = u32::from_be_bytes(addr);
    Ok(config::Route { addr: u32::to_be_bytes(net & mask), prefix_len })
}

impl PeerInfo {
    fn from_proto(p: &proto::RemotePeerConfig) -> Result<Self, ManagementError> {
        if p.wg_pub_key.is_empty() {
            return Err(ManagementError::Parse(
                "remote peer without wgPubKey (identity missing)".into(),
            ));
        }
        let mut allowed_ips = Vec::with_capacity(p.allowed_ips.len());
        for ip in &p.allowed_ips {
            allowed_ips.push(parse_masked_prefix_v4(ip)?);
        }
        let fqdn = (!p.fqdn.is_empty()).then(|| p.fqdn.clone());
        Ok(PeerInfo { wg_pub_key: p.wg_pub_key.clone(), allowed_ips, fqdn })
    }
}

impl ManagedRoute {
    /// `Ok(None)` = route skipped (unparseable/IPv6 network); the caller
    /// reports the raw network string (upstream log+skip,
    /// engine.go:1751-1753).
    fn from_proto(r: &proto::Route) -> Result<Option<Self>, ManagementError> {
        let network = match parse_masked_prefix_v4(&r.network) {
            Ok(net) => net,
            Err(_) => return Ok(None),
        };
        Ok(Some(ManagedRoute {
            id: r.id.clone(),
            network,
            domains: r.domains.clone(),
            net_id: r.net_id.clone(),
            network_type: r.network_type,
            peer: r.peer.clone(),
            metric: r.metric,
            masquerade: r.masquerade,
            keep_route: r.keep_route,
            skip_auto_apply: r.skip_auto_apply,
        }))
    }
}

impl DnsConfig {
    fn from_proto(d: &proto::DnsConfig) -> Result<Self, ManagementError> {
        let mut groups = Vec::with_capacity(d.name_server_groups.len());
        for g in &d.name_server_groups {
            let mut servers = Vec::with_capacity(g.name_servers.len());
            for ns in &g.name_servers {
                if !(0..=65535).contains(&ns.port) {
                    return Err(ManagementError::Parse(format!(
                        "name server port {} out of u16 range (ip {})",
                        ns.port, ns.ip
                    )));
                }
                servers.push(NameServer {
                    ip: ns.ip.clone(),
                    port: ns.port as u16,
                    ns_type: ns.ns_type,
                });
            }
            groups.push(NameServerGroup {
                name_servers: servers,
                primary: g.primary,
                domains: g.domains.clone(),
                search_domains_enabled: g.search_domains_enabled,
            });
        }
        Ok(DnsConfig { service_enable: d.service_enable, name_server_groups: groups })
    }
}

impl PeerSelfConfig {
    fn from_proto(p: &proto::PeerConfig) -> Self {
        PeerSelfConfig {
            address: (!p.address.is_empty()).then(|| p.address.clone()),
            interface_dns: (!p.dns.is_empty()).then(|| p.dns.clone()),
            fqdn: (!p.fqdn.is_empty()).then(|| p.fqdn.clone()),
            mtu: (p.mtu != 0).then_some(p.mtu),
            routing_peer_dns_resolution_enabled: p.routing_peer_dns_resolution_enabled,
            lazy_connection_enabled: p.lazy_connection_enabled,
        }
    }
}

impl NetworkMap {
    /// Convert a decoded `proto::NetworkMap` into the minimal model.
    ///
    /// Errors (strict, per the test contract): peer without `wgPubKey`,
    /// unparseable peer allowed-ip, out-of-range name-server port. Route
    /// networks that do not parse are NOT errors — they are skipped and
    /// reported (upstream behavior, engine.go:1751-1753).
    pub fn from_proto(map: &proto::NetworkMap) -> Result<Self, ManagementError> {
        let mut peers = Vec::with_capacity(map.remote_peers.len());
        for p in &map.remote_peers {
            peers.push(PeerInfo::from_proto(p)?);
        }
        let mut offline_peers = Vec::with_capacity(map.offline_peers.len());
        for p in &map.offline_peers {
            offline_peers.push(PeerInfo::from_proto(p)?);
        }
        let mut routes = Vec::with_capacity(map.routes.len());
        let mut skipped_routes = Vec::new();
        for r in &map.routes {
            match ManagedRoute::from_proto(r)? {
                Some(route) => routes.push(route),
                None => skipped_routes.push(r.network.clone()),
            }
        }
        let dns = match &map.dns_config {
            Some(d) => Some(DnsConfig::from_proto(d)?),
            None => None,
        };
        Ok(NetworkMap {
            serial: map.serial,
            peer: map.peer_config.as_ref().map(PeerSelfConfig::from_proto),
            peers,
            peers_is_empty: map.remote_peers_is_empty,
            offline_peers,
            routes,
            skipped_routes,
            dns,
        })
    }
}

impl NetbirdServers {
    /// Convert `SyncResponse.netbirdConfig` / `LoginResponse.netbirdConfig`.
    pub fn from_proto(cfg: &proto::NetbirdConfig) -> Self {
        NetbirdServers {
            stuns: cfg.stuns.iter().map(|h| h.uri.clone()).collect(),
            turns: cfg
                .turns
                .iter()
                .map(|t| TurnServer {
                    uri: t.host_config.as_ref().map(|h| h.uri.clone()).unwrap_or_default(),
                    user: t.user.clone(),
                    password: t.password.clone(),
                })
                .collect(),
            signal: cfg.signal.as_ref().map(|h| h.uri.clone()),
            relay: cfg.relay.as_ref().map(|r| RelayServers {
                urls: r.urls.clone(),
                token_payload: r.token_payload.clone(),
                token_signature: r.token_signature.clone(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message as _;

    fn route(network: &str) -> proto::Route {
        proto::Route {
            id: "r-1".into(),
            network: network.into(),
            network_type: 1,
            peer: "peer-relay-key".into(),
            metric: 10,
            masquerade: true,
            net_id: "net-1".into(),
            domains: vec!["corp.example.".into()],
            keep_route: false,
            skip_auto_apply: false,
        }
    }

    fn peer(key: &str, allowed: &[&str]) -> proto::RemotePeerConfig {
        proto::RemotePeerConfig {
            wg_pub_key: key.into(),
            allowed_ips: allowed.iter().map(|s| s.to_string()).collect(),
            ssh_config: None,
            fqdn: "host-1.netbird.cloud".into(),
            agent_version: "1.2.3".into(),
            lazy_state: 0,
        }
    }

    fn dns_config() -> proto::DnsConfig {
        proto::DnsConfig {
            service_enable: true,
            name_server_groups: vec![proto::NameServerGroup {
                name_servers: vec![
                    proto::NameServer { ip: "1.1.1.1".into(), ns_type: 0, port: 53 },
                    proto::NameServer { ip: "8.8.8.8".into(), ns_type: 1, port: 853 },
                ],
                primary: true,
                domains: vec!["example.com".into()],
                search_domains_enabled: false,
            }],
            // forwarder_port is deprecated upstream — not consumed here
            custom_zones: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn full_map_converts_with_fields_intact() {
        let map = proto::NetworkMap {
            serial: 42,
            peer_config: Some(proto::PeerConfig {
                address: "10.64.0.7".into(),
                dns: "100.100.0.1".into(),
                fqdn: "me.netbird.cloud".into(),
                mtu: 1380,
                routing_peer_dns_resolution_enabled: true,
                lazy_connection_enabled: true,
                ..Default::default()
            }),
            remote_peers: vec![peer("QUJDREVGRw==", &["10.30.30.1/32", "192.168.0.0/24"])],
            remote_peers_is_empty: false,
            routes: vec![route("172.16.0.0/12")],
            dns_config: Some(dns_config()),
            offline_peers: vec![peer("T0ZGTElORQ==", &[])],
            ..Default::default()
        };
        let model = NetworkMap::from_proto(&map).expect("converts");
        assert_eq!(model.serial, 42);
        let pc = model.peer.expect("peer config");
        assert_eq!(pc.address.as_deref(), Some("10.64.0.7"));
        assert_eq!(pc.interface_dns.as_deref(), Some("100.100.0.1"));
        assert_eq!(pc.mtu, Some(1380));
        assert!(pc.routing_peer_dns_resolution_enabled);
        assert!(pc.lazy_connection_enabled);

        assert_eq!(model.peers.len(), 1);
        let p = &model.peers[0];
        assert_eq!(p.wg_pub_key, "QUJDREVGRw==");
        assert_eq!(p.fqdn.as_deref(), Some("host-1.netbird.cloud"));
        // allowed_ips reuse the config::Route model
        assert_eq!(
            p.allowed_ips,
            vec![
                config::Route { addr: [10, 30, 30, 1], prefix_len: 32 },
                config::Route { addr: [192, 168, 0, 0], prefix_len: 24 },
            ]
        );
        assert!(!model.peers_is_empty);

        assert_eq!(model.offline_peers.len(), 1);
        assert_eq!(model.offline_peers[0].wg_pub_key, "T0ZGTElORQ==");

        assert_eq!(model.routes.len(), 1);
        let r = &model.routes[0];
        assert_eq!(r.id, "r-1");
        assert_eq!(r.network, config::Route { addr: [172, 16, 0, 0], prefix_len: 12 });
        assert_eq!(r.metric, 10);
        assert!(r.masquerade);
        assert_eq!(r.domains, vec!["corp.example."]);
        assert!(model.skipped_routes.is_empty());

        let dns = model.dns.expect("dns");
        assert!(dns.service_enable);
        assert_eq!(dns.name_server_groups.len(), 1);
        assert!(dns.name_server_groups[0].primary);
        assert_eq!(dns.name_server_groups[0].name_servers[0].port, 53);
        assert_eq!(dns.name_server_groups[0].name_servers[1].ns_type, 1);
    }

    /// An empty/default NetworkMap is a valid (empty) snapshot: no peers, no
    /// dns, no routes — and no skipped entries.
    #[test]
    fn empty_map_is_a_valid_snapshot() {
        let model = NetworkMap::from_proto(&proto::NetworkMap::default()).expect("empty ok");
        assert_eq!(model.serial, 0);
        assert!(model.peers.is_empty());
        assert!(model.routes.is_empty());
        assert!(model.dns.is_none());
        assert!(model.peer.is_none());
        assert!(model.skipped_routes.is_empty());
        // peers_is_empty preserved even when the list is unset (proto3
        // cannot distinguish [] from absent — the explicit flag carries it)
        assert!(!model.peers_is_empty);
    }

    /// Upstream masks route networks with host bits set
    /// (`prefix.Masked()`, engine.go:1755).
    #[test]
    fn route_network_host_bits_are_masked() {
        let map = proto::NetworkMap {
            routes: vec![route("10.1.2.3/24")],
            ..Default::default()
        };
        let model = NetworkMap::from_proto(&map).expect("converts");
        assert_eq!(model.routes[0].network, config::Route { addr: [10, 1, 2, 0], prefix_len: 24 });
        assert!(model.skipped_routes.is_empty());
    }

    /// Garbage and IPv6 route networks are skipped and REPORTED, not fatal —
    /// upstream logs and skips (engine.go:1751-1753); the whole snapshot
    /// must survive one bad route.
    #[test]
    fn bad_route_networks_are_skipped_and_reported() {
        let map = proto::NetworkMap {
            routes: vec![route("not-a-prefix"), route("fd00::/8"), route("10.9.0.0/16")],
            ..Default::default()
        };
        let model = NetworkMap::from_proto(&map).expect("converts despite skips");
        assert_eq!(model.routes.len(), 1);
        assert_eq!(model.routes[0].network, config::Route { addr: [10, 9, 0, 0], prefix_len: 16 });
        assert_eq!(model.skipped_routes, vec!["not-a-prefix".to_string(), "fd00::/8".to_string()]);
    }

    #[test]
    fn peer_without_wg_pub_key_is_an_error() {
        let map = proto::NetworkMap {
            remote_peers: vec![peer("", &["10.0.0.1/32"])],
            ..Default::default()
        };
        let err = NetworkMap::from_proto(&map).unwrap_err();
        assert!(matches!(&err, ManagementError::Parse(m) if m.contains("wgPubKey")), "{err:?}");
    }

    #[test]
    fn peer_allowed_ip_garbage_is_an_error() {
        let map = proto::NetworkMap {
            remote_peers: vec![peer("S09TVEE=", &["10.0.0.1/32", "oops"])],
            ..Default::default()
        };
        let err = NetworkMap::from_proto(&map).unwrap_err();
        assert!(matches!(&err, ManagementError::Parse(m) if m.contains("oops")), "{err:?}");
    }

    #[test]
    fn nameserver_port_out_of_range_is_an_error() {
        let mut dns = dns_config();
        dns.name_server_groups[0].name_servers[0].port = 70_000;
        let map = proto::NetworkMap { dns_config: Some(dns), ..Default::default() };
        let err = NetworkMap::from_proto(&map).unwrap_err();
        assert!(matches!(&err, ManagementError::Parse(m) if m.contains("70000")), "{err:?}");
    }

    /// Unknown NEW fields must be tolerated (forward compatibility): a
    /// hand-built NetworkMap encoding with an extra field 99 varint decodes
    /// and converts cleanly. Prost skips unknown tags by contract.
    #[test]
    fn unknown_future_fields_are_tolerated() {
        let mut map = proto::NetworkMap {
            serial: 7,
            remote_peers: vec![peer("RlVUVVJF", &[])],
            routes: vec![route("10.0.0.0/8")],
            ..Default::default()
        };
        let mut bytes = map.encode_to_vec();
        // append field 99, wire type 0 (varint): key varint (99<<3)|0 = 792
        // → [0x98, 0x06], value 1
        bytes.extend_from_slice(&[0x98, 0x06, 0x01]);
        let decoded = proto::NetworkMap::decode(bytes.as_slice()).expect("decodes with unknown field");
        assert_eq!(decoded, map, "unknown field dropped on decode, rest intact");
        map = decoded;
        let model = NetworkMap::from_proto(&map).expect("converts");
        assert_eq!(model.serial, 7);
    }

    #[test]
    fn netbird_servers_extraction() {
        let cfg = proto::NetbirdConfig {
            stuns: vec![proto::HostConfig { uri: "stun:stun.netbird.io:3478".into(), protocol: 0 }],
            turns: vec![proto::ProtectedHostConfig {
                host_config: Some(proto::HostConfig {
                    uri: "turn:turn.netbird.io:3478".into(),
                    protocol: 0,
                }),
                user: "u".into(),
                password: "p".into(),
            }],
            signal: Some(proto::HostConfig { uri: "signal.netbird.io:10000".into(), protocol: 1 }),
            relay: Some(proto::RelayConfig {
                urls: vec!["rels://relay.netbird.io:443".into()],
                token_payload: "payload".into(),
                token_signature: "signature".into(),
            }),
            flow: None,
            metrics: None,
        };
        let servers = NetbirdServers::from_proto(&cfg);
        assert_eq!(servers.stuns, vec!["stun:stun.netbird.io:3478"]);
        assert_eq!(servers.turns.len(), 1);
        assert_eq!(servers.turns[0].uri, "turn:turn.netbird.io:3478");
        assert_eq!(servers.turns[0].user, "u");
        assert_eq!(servers.signal.as_deref(), Some("signal.netbird.io:10000"));
        let relay = servers.relay.expect("relay");
        assert_eq!(relay.urls, vec!["rels://relay.netbird.io:443"]);
        assert_eq!(relay.token_payload, "payload");
        // default has no servers at all
        assert_eq!(NetbirdServers::from_proto(&proto::NetbirdConfig::default()), NetbirdServers::default());
    }
}
