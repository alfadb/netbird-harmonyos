# Duplicate route audit

**Status: v0.0.8 build, install, and bounded route capture completed.** This records one authorized upgrade install and one sample of the existing connection flow. It is not a claim that either peer forwarded LAN traffic.

## Build and install

- Signed artifact: `netbird-0.0.8-signed.hap`; `versionCode=8`, `versionName=0.0.8`; SHA-256 `2b65fca683ec984ba45815754782e1d6224598bd2152b53706a0c845e04282e3`.
- Build, signing, and signature verification each completed with exit code 0. The signature verifier reported success.
- Device package-manager state changed from `0.0.6` / code 6 to `0.0.8` / code 8 after one `hdc install -r` (`install bundle successfully`). Post-install `updateTime=1791300493725`; `firstInstallTime=1789309788737` was unchanged, confirming an upgrade.
- Capture time: 2026-10-06 23:36 CST (+0800). The app was launched and the existing connect control tapped once. No settings, auth mode, VPN configuration, or route behavior were changed.

## Route evidence

- Session `1791300887859-55966-0`, PID `55966`. `N6_ROUTE_MAP` serial `302` has two rows, both `net_id=HostInternal`, `network=192.168.50.0/24`, `network_type=1`, `masquerade=true`, `keep_route=true`, `skip_auto_apply=false`:

| Route ID | Peer (abbreviated) | Peer mapping | Metric | Index / total |
|---|---|---|---:|---:|
| `d29f0ml27eas73a9rqig%3Ad3hj7d527eas73cfdij0` | `F0GVt4oE...` | RPi, `100.108.162.237` | 200 | 0 / 2 |
| `d29f0ml27eas73a9rqig%3Ad29evt527eas73a9rqh0` | `t7tldQA3...` | NetHostDebian, `100.108.171.38` | 100 | 1 / 2 |

- These are distinct route IDs and peer keys advertising the same prefix. `N6_WG_DEVICE|route-peer` and `N6_WG_DEVICE|route-selection` named the metric-100 peer with `live=false`, `reason=routes-updated`; the selected peer is a candidate at that observation, not proof of a live LAN route owner. Subsequent `session-established` events include both route peers, but do not establish LAN forwarding or route ownership. No later selection or `live=true` record appeared in the bounded excerpt.
- The map record contract is `serial/index/total/id/net_id/network/peer/metric/network_type/masquerade/keep_route/skip_auto_apply`. Owner-change and route-selection records are per-map evidence; they have no selection serial, so correlate them to map serial `302` by the captured session and destination prefix. Do not infer a missing serial or use 10 ms periodic logs for this audit.
- The raw management list shows two same-prefix routes while the WG summary exposes one prefix. Effective per-peer AllowedIPs ownership excludes a backup subnet, so an earlier UI screenshot cannot rule out the RPi as an HA candidate. Neither the UI list nor this capture establishes management-side intent, OS route installation, a direct LAN path, or which peer forwarded traffic.
- Connection samples reported `conn_state=connected` at 23:34:54 and 23:35:54 CST; transport samples reported `icesock_ready=true`, `relay_ready=true`, and `supply_installed=true`. This shows the VPN remained connected after the tap, not that the LAN route was live.

## Verification and limits

- The route-audit focused tests (16) and persistent-log test (1) passed as previously recorded; they were not rerun for this capture. The v0.0.7 regression result remains prior evidence, not a v0.0.8 rerun.
- No source edits, build, test, or device operation was performed for this documentation update. No account login, SSO test, credential access, or configuration change is claimed.
- Sanitized capture: `/home/worker/n2h-kit/status-20261001/v008-route-audit-20261006-233619/REPORT.md` and `public-route-audit.txt`. Build/signing manifest: `/home/worker/n2h-kit/app-builds/route-audit-v008-20261006-232146/MANIFEST.md`.
