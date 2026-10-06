//! Bounded diagnostic formatting for accepted management route rows.

use crate::network_map::ManagedRoute;

const OPAQUE_FIELD_MAX: usize = 96;

/// Format one accepted map route while its management identity and policy
/// fields are still available. Opaque strings are escaped and capped so a
/// malformed server value cannot create an unbounded/multiline log record.
pub fn managed_route_line(serial: u64, index: usize, total: usize, route: &ManagedRoute) -> String {
    let (addr, prefix) = (route.network.addr, route.network.prefix_len);
    let id = bounded_opaque(&route.id);
    let net_id = bounded_opaque(&route.net_id);
    let peer = bounded_opaque(&route.peer);
    format!(
        "N6_ROUTE_MAP|serial={serial}|index={index}|total={total}|id={id}|net_id={net_id}|network={}.{}.{}.{}/{}|peer={peer}|metric={}|network_type={}|masquerade={}|keep_route={}|skip_auto_apply={}",
        addr[0], addr[1], addr[2], addr[3], prefix, route.metric, route.network_type,
        route.masquerade, route.keep_route, route.skip_auto_apply,
    )
}

fn bounded_opaque(value: &str) -> String {
    const MARKER: &str = "~truncated";
    const VALUE_MAX: usize = OPAQUE_FIELD_MAX - MARKER.len();
    let mut escaped = String::with_capacity(value.len().min(OPAQUE_FIELD_MAX));
    let mut truncated = false;
    for byte in value.bytes() {
        let token = if byte.is_ascii_alphanumeric()
            || matches!(byte, b'.' | b'_' | b'-' | b'/' | b'+' | b'=')
        {
            let mut token = String::with_capacity(1);
            token.push(byte as char);
            token
        } else {
            format!("%{byte:02X}")
        };
        if escaped.len() + token.len() > VALUE_MAX {
            truncated = true;
            break;
        }
        escaped.push_str(&token);
    }
    if !truncated && escaped.len() < value.len() {
        truncated = true;
    }
    if truncated {
        escaped.push_str(MARKER);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, network_map::ManagedRoute};

    fn route(id: &str, peer: &str) -> ManagedRoute {
        ManagedRoute {
            id: id.into(),
            network: config::Route {
                addr: [192, 168, 50, 0],
                prefix_len: 24,
            },
            domains: vec![],
            net_id: "net-lan".into(),
            network_type: 1,
            peer: peer.into(),
            metric: 10,
            masquerade: true,
            keep_route: true,
            skip_auto_apply: false,
        }
    }

    #[test]
    fn accepted_rows_distinguish_route_ids_and_router_keys() {
        let first = managed_route_line(71, 0, 2, &route("route-a", "FyLw4PSBg1hjr+9/x0="));
        let second = managed_route_line(71, 1, 2, &route("route-b", "FyLw4PSBg1hjr+9/x0="));
        let other_peer = managed_route_line(71, 1, 2, &route("route-b", "another-router"));

        assert!(first.contains("serial=71|index=0|total=2|id=route-a|net_id=net-lan"));
        assert!(first.contains("network=192.168.50.0/24|peer=FyLw4PSBg1hjr+9/x0="));
        assert!(first.contains(
            "metric=10|network_type=1|masquerade=true|keep_route=true|skip_auto_apply=false"
        ));
        assert_ne!(
            first, second,
            "same prefix/router with different route IDs must remain distinct"
        );
        assert_ne!(
            second, other_peer,
            "same route ID/prefix with another router must remain distinct"
        );
    }

    #[test]
    fn opaque_values_escape_delimiters_and_mark_bounded_truncation() {
        let escaped = bounded_opaque("route|one\nnext");
        assert_eq!(escaped, "route%7Cone%0Anext");

        let long = bounded_opaque(&"x".repeat(4096));
        assert_eq!(long.len(), OPAQUE_FIELD_MAX);
        assert!(long.ends_with("~truncated"));

        let line = managed_route_line(1, 0, 1, &route(&"i".repeat(4096), &"k".repeat(4096)));
        assert!(
            line.len() < 1024,
            "phone_log cap is 1024 characters: {}",
            line.len()
        );
        assert!(line.contains(&format!("id={}", bounded_opaque(&"i".repeat(4096)))));
        assert!(line.contains(&format!("peer={}", bounded_opaque(&"k".repeat(4096)))));
    }
}
