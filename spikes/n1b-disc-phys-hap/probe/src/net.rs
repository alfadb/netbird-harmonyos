//! Self-synthesized raw IPv4/UDP packet builders and parsers (D4/D5/D8).
//! Nothing here goes through BoringTun (gate-plan :302 — the crypto data plane
//! is never exercised; probe packets are frozen raw IPv4 so that crypto failures
//! can never masquerade as platform behavior).

// ---------------------------------------------------------------------------
// addresses (frozen per matrix entry)
// ---------------------------------------------------------------------------

pub const ADDR_TUN_LOCAL: [u8; 4] = [10, 99, 0, 1]; // MR*: tun address (D5 dst)
pub const ADDR_PEER: [u8; 4] = [10, 99, 0, 2]; // MR*: D4 send dest / D5 src
pub const ADDR_MB1_LOCAL: [u8; 4] = [192, 0, 2, 1]; // MB1: E3-form tun address
pub const ADDR_MB1_PEER: [u8; 4] = [192, 0, 2, 2]; // MB1: D4 send dest / D5 src

pub fn dst_peer(mb1: bool) -> [u8; 4] {
    if mb1 { ADDR_MB1_PEER } else { ADDR_PEER }
}
pub fn dst_local(mb1: bool) -> [u8; 4] {
    if mb1 { ADDR_MB1_LOCAL } else { ADDR_TUN_LOCAL }
}

// ---------------------------------------------------------------------------
// IPv4 checksum (gate-plan D5: one's-complement sum of 16-bit words over the
// 20-byte header with the checksum field zeroed, odd byte zero-padded)
// ---------------------------------------------------------------------------

pub fn ipv4_checksum(header: &[u8]) -> u16 {
    debug_assert!(header.len() >= 20);
    let mut sum: u32 = 0;
    let mut words = header[..20].chunks_exact(2);
    for (i, w) in words.by_ref().enumerate() {
        if i == 5 {
            continue; // checksum field position (bytes 10-11) is zeroed
        }
        sum = sum.wrapping_add(((w[0] as u32) << 8) | w[1] as u32);
    }
    let rem = words.remainder();
    if rem.len() == 1 {
        sum = sum.wrapping_add((rem[0] as u32) << 8);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

// ---------------------------------------------------------------------------
// packet builders
// ---------------------------------------------------------------------------

/// 16-byte D4 control payload: "N1DISCD4" | 0x01 | seq(BE16) | 0x5A x5.
pub fn d4_payload(seq: u16) -> [u8; 16] {
    let mut p = [0u8; 16];
    p[..8].copy_from_slice(b"N1DISCD4");
    p[8] = 0x01;
    p[9] = (seq >> 8) as u8;
    p[10] = (seq & 0xff) as u8;
    for b in p[11..16].iter_mut() {
        *b = 0x5a;
    }
    p
}

/// 44-byte frozen D5 packet (D5 layout, gate-plan :576-578).
/// IPv4: v4/IHL5, TOS 0, total_length 44, id 0x0001, flags 0, frag 0, TTL 64,
/// proto 17, checksum recomputed, src/dst per matrix entry.
/// UDP: sport 47001, dport 47002, length 24, checksum 0.
/// payload: "N1DISCD5" | 0x01 | seq(BE16) | 0x5A x5.
pub fn d5_packet(seq: u16, mb1: bool) -> [u8; 44] {
    let mut pkt = [0u8; 44];
    build_ipv4_udp_header(&mut pkt, 44, 0x0001, dst_peer(mb1), dst_local(mb1));
    pkt[28..36].copy_from_slice(b"N1DISCD5");
    pkt[36] = 0x01; // round
    pkt[37] = (seq >> 8) as u8;
    pkt[38] = (seq & 0xff) as u8;
    for b in pkt[39..44].iter_mut() {
        *b = 0x5a;
    }
    pkt
}

/// Build a D8a/D8b ladder packet of total length L (gate-plan :591-592):
/// UDP length = L-20; payload = "N1DISCD8"(8B) + len(BE16 = L) + 0x5A fill.
pub fn d8_packet(l: usize, id: u16, mb1: bool) -> Vec<u8> {
    let mut pkt = vec![0u8; l];
    build_ipv4_udp_header(&mut pkt, l, id, dst_peer(mb1), dst_local(mb1));
    pkt[28..36].copy_from_slice(b"N1DISCD8");
    pkt[36] = (l >> 8) as u8;
    pkt[37] = (l & 0xff) as u8;
    for b in pkt[38..].iter_mut() {
        *b = 0x5a;
    }
    pkt
}

/// Write the 20-byte IPv4 header + 8-byte UDP header (checksum recomputed over
/// the header with the checksum field zeroed; UDP checksum = 0). Requires
/// pkt.len() >= 28.
fn build_ipv4_udp_header(pkt: &mut [u8], total: usize, id: u16, src: [u8; 4], dst: [u8; 4]) {
    pkt[0] = 0x45; // version 4, IHL 5
    pkt[1] = 0; // TOS
    pkt[2] = (total >> 8) as u8;
    pkt[3] = (total & 0xff) as u8;
    pkt[4] = (id >> 8) as u8;
    pkt[5] = (id & 0xff) as u8;
    pkt[6] = 0; // flags
    pkt[7] = 0; // frag off
    pkt[8] = 64; // TTL
    pkt[9] = 17; // proto UDP
    pkt[10] = 0; // checksum zero during computation
    pkt[11] = 0;
    pkt[12..16].copy_from_slice(&src);
    pkt[16..20].copy_from_slice(&dst);
    let ck = ipv4_checksum(&pkt[..20]);
    pkt[10] = (ck >> 8) as u8;
    pkt[11] = (ck & 0xff) as u8;
    // UDP header at 20
    pkt[20] = (47001 >> 8) as u8;
    pkt[21] = (47001 & 0xff) as u8;
    pkt[22] = (47002 >> 8) as u8;
    pkt[23] = (47002 & 0xff) as u8;
    let ulen = total - 20;
    pkt[24] = (ulen >> 8) as u8;
    pkt[25] = (ulen & 0xff) as u8;
    pkt[26] = 0; // UDP checksum 0 (IPv4-legal)
    pkt[27] = 0;
}

// ---------------------------------------------------------------------------
// frame parsing (D4 double-offset, U3 partition)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Ipv4Hdr<'a> {
    pub total_length: u16,
    pub proto: u8,
    pub src: [u8; 4],
    pub dst: [u8; 4],
    pub udp_dport: u16,
    pub udp_payload: &'a [u8],
}

/// Try to parse an IPv4 header at `off` in a read frame. Parsability per the
/// frozen rule = version 4 and IHL 5 (plus enough bytes for the 20-byte header);
/// UDP fields are extracted when proto == 17.
pub fn parse_ipv4_at(frame: &[u8], off: usize) -> Option<Ipv4Hdr<'_>> {
    if frame.len() < off + 20 {
        return None;
    }
    let v = frame[off] >> 4;
    let ihl = frame[off] & 0x0f;
    if v != 4 || ihl != 5 {
        return None;
    }
    let total_length = ((frame[off + 2] as u16) << 8) | frame[off + 3] as u16;
    let proto = frame[off + 9];
    let mut src = [0u8; 4];
    src.copy_from_slice(&frame[off + 12..off + 16]);
    let mut dst = [0u8; 4];
    dst.copy_from_slice(&frame[off + 16..off + 20]);
    let mut udp_dport = 0u16;
    let mut udp_payload: &[u8] = &[];
    if proto == 17 && frame.len() >= off + 28 {
        udp_dport = ((frame[off + 22] as u16) << 8) | frame[off + 23] as u16;
        let end = core::cmp::min(frame.len(), off + 20 + 8 + 16);
        udp_payload = &frame[off + 28..end];
    }
    Some(Ipv4Hdr {
        total_length,
        proto,
        src,
        dst,
        udp_dport,
        udp_payload,
    })
}

/// Which offsets parse as IPv4 for this frame -> "0" / "4" / "both" / "none".
pub fn offsets_parsable(frame: &[u8]) -> (bool, bool, &'static str) {
    let o0 = parse_ipv4_at(frame, 0).is_some();
    let o4 = parse_ipv4_at(frame, 4).is_some();
    let s = match (o0, o4) {
        (true, true) => "both",
        (true, false) => "0",
        (false, true) => "4",
        (false, false) => "none",
    };
    (o0, o4, s)
}

/// U3 prefix classification per the frozen 2x2 mutually-exclusive partition
/// (gate-plan :551-561). Returns (classification, prefix4 bytes).
pub fn u3_prefix_class(frame: &[u8], o0: bool, o4: bool) -> (&'static str, [u8; 4]) {
    let mut p4 = [0u8; 4];
    if frame.len() >= 4 {
        p4.copy_from_slice(&frame[..4]);
    }
    match (o0, o4) {
        (false, false) => ("unparsable", p4),
        (true, false) => ("no-prefix", p4),
        (false, true) => {
            // tun_pi-like: first 2 bytes flags + next 2 bytes BE proto 0x0800
            if p4[0] == 0 && p4[1] == 0 && p4[2] == 0x08 && p4[3] == 0x00 {
                ("tun_pi-like", p4)
            } else {
                ("other-prefix", p4)
            }
        }
        (true, true) => ("ambiguous", p4),
    }
}

/// u3_readlen_vs_total_length classification (five values).
pub fn readlen_vs_total(frame: &[u8], off: usize) -> &'static str {
    match parse_ipv4_at(frame, off) {
        Some(h) => {
            let tl = h.total_length as usize;
            if frame.len() == tl {
                "equal"
            } else if frame.len() > tl {
                "readlen>total_length"
            } else {
                "readlen<total_length"
            }
        }
        None => "unparsable",
    }
}
