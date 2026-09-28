# N2 evidence switch: pre-freeze diagnostic smoke (2026-09-28)

This is diagnostic code, not a frozen N2 oracle or a device authorization. No device measurement was made here. The preregistration draft `n2-device-evidence-preregistration-draft-v2-20260928.md` still requires independent review, operator approval, and a separately authorized smoke before freezing.

## Ticket and safeguards

The existing EntryAbility `n2h_arm_options` private ticket may include:

```json
{
  "endpoint_exclusions_enabled": false,
  "n2_evidence": {
    "trusted_self": true,
    "capture_32": ["119.29.29.119"],
    "hdc_control_ip": "192.168.50.1",
    "allow_hdc_overlap": false
  },
  "protect_probe": { "target": "119.29.29.119:53", "count": 1, "mode": "none" }
}
```

An absent `n2_evidence` key is inert. An armed ticket sets `trustedApplications` to this App alone and removes the template LAN exclusion and endpoint exclusions; each specified IPv4 receives a non-excluded `/32`, in the initial and recreate configs. The capture list is independent of legacy S6 `diag_capture_route`, which must be off. A true S2 `diag_blocked_self`, true S6, `forceDefaultRoute=true`, explicit `endpoint_exclusions_enabled=true`, malformed/duplicate addresses, and a captured `hdc_control_ip` without `allow_hdc_overlap=true` refuse startup. If the hdc address is unknown, the code cannot infer it: record it explicitly before the authorized smoke; do not treat omission as proof of non-overlap. Even with an explicit overlap override, the required nonlisted-app/hdcd device check must pass before relying on the scheme. The default route is held even if the peer becomes ready, and the initial/recreate configuration rejects a remaining default route. Any route masked by an exclusion fails before create. A recreate is marked `N2_EVIDENCE_RECREATE_DETECTED`; this smoke is primarily about the first generation, not E-cell acceptance.

The existing `protect_probe` works independently of S6. Run separate authorized windows with identical frozen destination and `mode:"none"` then `mode:"protect"`; do not conflate a sent request with an observed packet. When `n2_evidence` is armed, the `N2_TUN_DEST|scope=session` counter is attached before the TUN feed and survives collector ARM/CLOSE for the whole connector/VPN session (including a TUN replacement). It counts from-OS frames by destination IPv4, IP protocol and destination port; every 5 seconds it logs cumulative `frames`, since-last-output `delta`, monotonic `mono_ms` and up to 8 distinct source IPv4 addresses per group, if counts changed, and otherwise at least once every 30 seconds. Each destination has at most 64 named protocol/port groups plus one `proto=0|port=0` overflow group. A `proto=any|port=any|frames=0|delta=0` line is a zero-hit heartbeat, not a sent/received-packet receipt. Compare cumulative counts and their deltas across the actual send intervals in (a), (b) and (c), including intervals outside the collector window; unchanged totals alone do not prove a socket sent. Collector CLOSE still emits the legacy `N2_TUN_DEST` window-only summary without `scope=session`, and `frames=0` there only describes that short window. Both logs are diagnostic (`diag=true|is_evidence=false`) and cannot identify UID from TUN packets or change frozen evidence bytes. `N2_PROC_NET` probes `/proc/net/{tcp,udp,tcp6,udp6}` and `/proc/self/net/{tcp,udp,tcp6,udp6}` inside the extension, logging readability, line count and UID from `/proc/self/status` if available, not contents; permission denial is a diagnostic result, not a gate pass. The collector document's `unsat_note` is diagnostic context only; protect status and acceptance criteria come from the current preregistration, not an old platform-permission assumption.

## Device smoke still required

Under a separate diagnostic authorization, retain original logs and network-side receipts for (a) an actual unprotected App socket entering TUN, (b) the same destination from a protected App socket not entering TUN while physical/remote receipt confirms send, (c) an actual nonlisted app or hdcd send not entering TUN with receipt and capture positive control, and (d) successful platform create with S2/S6 off. If hdc and peer/relay overlap, (c) must specifically exercise hdcd. Log route rows, applied generations, `trustedApplications`, `N2_TUN_DEST`, `N2_PROC_NET`, collector output and the target-side results for each arm. The code does not emit trustedApplications as an SDK-returned applied-list receipt; verify on-device behavior and config creation rather than treating an intention log as acceptance. Recreate, route drift, missing physical receipts or unobserved sends cannot be promoted to a pass. No device command is authorized by this document.
