# N2 evidence switch: preregistration v4 T3/T5a (2026-09-28)

The N2 preregistration v4 is frozen, but this implementation is diagnostic code, not a device authorization or a completed N2 verdict. No device measurement was made here. Production results require the separately authorized campaign, independent receipts and the frozen `n2-device-evidence-preregistration-draft-v2-20260928.md` gates.

## Ticket and safeguards

The existing EntryAbility `n2h_arm_options` private ticket may include:

```json
{
  "endpoint_exclusions_enabled": false,
  "n2_evidence": {
    "trusted_self": true,
    "capture_32": ["119.29.29.119"],
    "outer_destinations": ["tcp/119.29.29.119:443", "udp/119.29.29.119:53"],
    "hdc_control_ip": "192.168.50.1",
    "allow_hdc_overlap": false
  },
  "protect_probe": { "target": "119.29.29.119:53", "count": 1, "mode": "none" }
}
```

An absent `n2_evidence` key is inert. An armed ticket sets `trustedApplications` to this App alone and removes the template LAN exclusion and endpoint exclusions; each specified IPv4 receives a non-excluded `/32`, in the initial and recreate configs. An armed ticket declares **no** platform DNS server: `withN2EvidenceRoutes` returns `dnsServers: []` and the platform mapper then omits `VpnConfig.dnsAddresses` instead of submitting an empty array (N2 evidence DNS decision (a), 2026-09-28). Rationale: the App's own outer resolution never uses the platform DNS — initial management/signal names resolve on the physical network before `create()` (`resolveManagementIp`), post-create/relay/STUN lookups go through the core's protected-DNS fd path (`resolveProtectedIp`), and the D row forces a protected query rather than the system resolver — while a declared resolver IP that is also a `capture_32` destination (e.g. the Wi-Fi resolver, or `223.5.5.5` listed as a D-row target) would let the platform route our own resolver query into the TUN, where the campaign-wide from-OS destination scan counts it as an outer frame to a frozen destination: a false hit. Option (b) — keep the field and refuse at arm time when a server collides with `capture_32` — was rejected because the DNS set is only final at `create()`: the core network-map snapshot is applied after the ticket is armed, so a check over the deployment default (`8.8.8.8`) would miss a management-advertised server. Ordinary unarmed configs keep their `dnsAddresses` shape unchanged. The capture list is independent of legacy S6 `diag_capture_route`, which must be off. A true S2 `diag_blocked_self`, true S6, `forceDefaultRoute=true`, explicit `endpoint_exclusions_enabled=true`, malformed/duplicate addresses, and a captured `hdc_control_ip` without `allow_hdc_overlap=true` refuse startup. If the hdc address is unknown, the code cannot infer it: record it explicitly before the authorized smoke; do not treat omission as proof of non-overlap. Even with an explicit overlap override, the required nonlisted-app/hdcd device check must pass before relying on the scheme. The default route is held even if the peer becomes ready, and the initial/recreate configuration rejects a remaining default route. Any route masked by an exclusion fails before create. A recreate is marked `N2_EVIDENCE_RECREATE_DETECTED`; this smoke is primarily about the first generation, not E-cell acceptance.

The existing `protect_probe` works independently of S6. Run separate authorized windows with identical frozen destination and `mode:"none"` then `mode:"protect"`; do not conflate a sent request with an observed packet. When `n2_evidence` is armed, the `N2_TUN_DEST|scope=session` counter is attached before the TUN feed and survives collector ARM/CLOSE for the whole connector/VPN session (including a TUN replacement). It counts from-OS frames by destination IPv4, IP protocol and destination port; every 5 seconds it logs cumulative `frames`, since-last-output `delta`, monotonic `mono_ms` and up to 8 distinct source IPv4 addresses per group, if counts changed, and otherwise at least once every 30 seconds. Each destination has at most 64 named protocol/port groups plus one `proto=0|port=0` overflow group. A `proto=any|port=any|frames=0|delta=0` line is a zero-hit heartbeat, not a sent/received-packet receipt. Compare cumulative counts and their deltas across the actual send intervals in (a), (b) and (c), including intervals outside the collector window; unchanged totals alone do not prove a socket sent. Collector CLOSE still emits the legacy `N2_TUN_DEST` window-only summary without `scope=session`, and `frames=0` there only describes that short window. Both logs are diagnostic (`diag=true|is_evidence=false`) and cannot identify UID from TUN packets or change frozen evidence bytes. `N2_PROC_NET` probes `/proc/net/{tcp,udp,tcp6,udp6}` and `/proc/self/net/{tcp,udp,tcp6,udp6}` inside the extension, logging readability, line count and UID from `/proc/self/status` if available, not contents; permission denial is a diagnostic result, not a gate pass. The collector document's `unsat_note` is diagnostic context only; protect status and acceptance criteria come from the current preregistration, not an old platform-permission assumption.

## T3 positive controls and sandwich

`n2_evidence.outer_destinations` is required when armed: a nonempty list (maximum 128) of unique `tcp/IP:port` or `udp/IP:port` targets, each IP in `capture_32`. The core refuses missing/invalid targets before TUN attach, freezes the exact protocol/IP/port set for this session, and emits `N2_TUN_LEAK|verdict=fail` immediately on a matching *non-control* from-OS frame regardless of source IP. The shell and core keep default-unarmed behavior unchanged. An incomplete outer set is **not** proof of absence; compare it with every observed production endpoint throughout the campaign and stop as `UNSAT` on drift.

`NetBirdN2Control.ets` exports the explicit `withN2Sandwich(capture32, target, proto, eventId, production)` convention. Only an already-armed session may call it, and `production` must be one real production send callback, not a synthetic substitute. It sends and waits for a **pre** HIT, invokes and awaits that callback, then sends and waits for a **post** HIT; exceptions or MISS abort that event. No production path invokes `withN2Sandwich` automatically; the armed-only **P5** orchestration below uses the same pre → production → post ordering through the exported `sendAndWaitN2Control` leg primitive (see「I/Ws sandwich orchestration (P5)」). Each `sendN2Control` opens an unprotected fd through the same production shell opener (`mgmtSocketOpen` for TCP, `iceSocketOpen` for UDP), registers before I/O, and closes the shell fd in `finally`. TCP matches a registered local-port SYN during a bounded window; UDP matches the exact `N2CTRL:<marker>` payload and destination. Neither needs a listener or a special test port. Native `service_tun` consumes a matched frame before either the collector tee or WG encryption, logs `N2_CONTROL_HIT`/`N2_CONTROL_MISS`, and maintains independent `N2_TUN_FRAMES` control/production counts. Every `N2_CONTROL_HIT`/`_MISS`/`_SENT` line carries both the REQUEST `id=` and the matched `marker=` (identical by construction, N2 T7a item 2), so a hit can be looked up by the id the operator asked for. `n2_control_status` reports cumulative `leaks`; the sandwich aborts when it sees a nonzero value. A negative/protected socket observation cannot be accepted without both ordered HITs for that event and independent send receipts.

`N2_TUN_GAP` records the pre-attach period, TUN replacements and the tee detach (monotonic bounds as `from_mono=`/`to_mono=`, N2 T7a item 3). The session-period `N2_TUN_TEE_STATS|frames=…|decode_fail=…|dropped=…|mono_ms=…` line reports the tee's own continuity (`frames = decode_fail + dropped + Σ N2_TUN_DEST.frames`): a non-zero `decode_fail` makes the interval incomplete. A recorded gap overlapping a production event, a failed read/decode, missing control, unknown source attribution, unmatched target or independently observed packet loss makes the relevant interval `invalid/UNSAT`; a zero counter is not a pass. The core does not prove kernel queue integrity or physical receipt, and the bounded per-event control window cannot substitute for independent full-campaign capture/drop accounting. T3 tests use fake TUN frames only and perform no device/network campaign.

## Outer-socket lifecycle receipts (T2/T4)

Only an accepted `n2_evidence` ticket arms `N2_SOCK|` logging; an absent or refused ticket leaves the old log stream unchanged. `phase=open` and `phase=protect_begin` are emitted by the ArkTS protect wrapper for an opened fd, `phase=protect_settle` records the bounded result and `elapsed_ms`. N2 T7a item 6: the shell `phase=open`/`phase=protect_begin`/`phase=protect_settle` lines now spell the generation field `protect_generation=` (the frozen E1 name), and `protect_settle` carries `status=success|failed` instead of `result=`, plus `local=` (the socket's own `getsockname` name, returned by `n2_socket_trace_protect`; `unknown` when the number cannot be inspected — never an invented address). The `status`/`protect_generation` vocabulary is therefore the core `N2_SOCK` one and matches the frozen criterion `phase=protect_settle|status=success`. Since 2026-09-30 the CORE also emits its own `N2_SOCK|phase=protect_settle|kind=Wb` row on every armed `Wb` takeover (`n2_socket_trace::adopted`, i.e. only after the duplicated descriptor was validated; whether or not a C/D plan measures the class): `fd` is the shell original, `dup_fd` the validated core-owned descriptor, `proto=17` (the frozen E1 spelling), `protect`/`status` the ACTUAL protect verdict (a failed protect reports `failed` and publishes no live identity), and `protect_mono_ms` the core's `sys::mono_ms()` at the protect verdict — the field comparable with the tee anchors for the frozen E1 ordering; the row deliberately carries NO wall field, because the only wall available at emission is the later takeover clock and it must not be presented as the protect wall. `phase=first_io_begin` is recorded immediately before TCP `connect` or UDP `send`; the core's `phase=first_io|status=connected|sent|failed` receipt is recorded after that call returns. A reused ICE/WG fd records later sends as `phase=subsequent_io_begin` and `phase=subsequent_io` with the same original/dup fd chain and a new destination and completion time. N2 T7a item 4: every `phase=subsequent_io` completion carries `event_id=<id>` — the A-column event the write belongs to while an event window is open (`none` outside every window) — and a write inside an open window is ALWAYS receipted, bypassing the first/every-128th/30-s throttle, so the one production write an A event causes can never be missing from the log. `first_mono_ms` and `first_wall_ms` on completion receipts are **completion timestamps**, not wire timestamps. `fd` is the shell original, `dup_fd` is the core-owned descriptor. `request_id` is the monotonic supply ID for on-demand sockets; initial and spare sockets use the extension request ID. `protect_generation` and `net_id` reflect the shell snapshot at protect completion; `net_id=0` means unavailable, not a verified network. The receipt also gives `local` from `getsockname` after the call, `remote` as numeric address and port, `proto`, `kind` (`M` management, `S` signal, `R` relay, `D` DNS, `I` ICE, `Wb` WG bound socket, `Ws` WG supplied socket), and `detail`. `local=unknown` is not evidence of a physical source. The logs are `diag=true|is_evidence=false` and do not establish a successful protect independently of the platform's actual behavior. A local IPv4 in `100.108.0.0/16` additionally emits `N2_SOCK_OVERLAY_LOCAL`; it is a stop/fail signal, not a substitute for network-side observations. The shell's `VPN_WG_DATAPLANE_FED` line (T7a item 6) carries `mono_ms=` from the core's `connector_tun_fd_feed` reply and `wall_ms=Date.now()`, so it is alignable with the core's `N7_WG_FEED|adopted|…|mono_ms=` from the same call; `N7_WG_FEED|adopted` itself now also reports `endpoints=` and `egress_replayed=`, and `N2_FD_SEND` keeps exactly `packets`/`bytes`/`mono_ms`.

Pair M/S TCP with the matching server access log using destination IP:port, device local source IP:port after connect, event time and a bounded clock error; if NAT intervenes, require the frozen router/AP mapping to bridge the before/after tuple. Pair R only with an enabled relay access log and its actual destination, not a guessed peer address. For D, require the protected DNS fd's sent query and same-ID response, adjacent from-OS positive-control hit, campaign-wide zero leakage, physical handle and actual resolver destination under the frozen no-exit-node condition; an independently available resolver query log strengthens this but is not mandatory. The ordinary `protected-dns` line has the hostname; `N2_SOCK` does not log it. For I, `detail=stun_txn_<hex>` identifies the 96-bit STUN transaction; `N2_SOCK_STUN` records the decoded XOR-MAPPED-ADDRESS, server, local tuple and the same transaction ID. Verify whether coturn actually records transaction IDs before pairing by ID; absent that field, uniquely pair using destination, time window, response-side ID and independent metadata, or mark `invalid/UNSAT`. Once uniquely paired, a coturn remote tuple differing from XOR-MAPPED-ADDRESS is `UNSAT` and a risk event, not a negative-packet pass. The mapped address is a NAT observation, **not** a claimed device local IP. Public-side tuples alone are insufficient on shared LAN/hairpin/cellular NAT: preserve the AP/router or operator mapping and both sides' clock calibration. Three-way attribution needs the device fd/dup chain, physical-interface/network-side send and independent target receipt, together with the required TUN positive control. Missing required access logs for M/S/R or Wb/Ws, unmatched I transactions, ambiguous NAT, unknown local tuple, or absent per-socket protect/first-send logs make the corresponding cell `invalid/UNSAT`, never a negative-packet pass. Wb/Ws send completion requires the WG data-plane path; shell protect alone is not a send receipt.


Under a separate diagnostic authorization, retain original logs and network-side receipts for (a) an actual unprotected App socket entering TUN, (b) the same destination from a protected App socket not entering TUN while physical/remote receipt confirms send, (c) an actual nonlisted app or hdcd send not entering TUN with receipt and capture positive control, and (d) successful platform create with S2/S6 off. If hdc and peer/relay overlap, (c) must specifically exercise hdcd. Log route rows, applied generations, `trustedApplications`, `N2_TUN_DEST`, `N2_PROC_NET`, collector output and the target-side results for each arm. The code does not emit trustedApplications as an SDK-returned applied-list receipt; verify on-device behavior and config creation rather than treating an intention log as acceptance. Recreate, route drift, missing physical receipts or unobserved sends cannot be promoted to a pass. No device command is authorized by this document.

## B-stream stimulus (T5a, 2026-09-28)

Preregistration v4 §4 B column / §7 T5a, frozen decision 4. This is the
**default-off, evidence-switch-gated outer-stream stimulus**: it is inert and
unreachable until an accepted `n2_evidence` ticket armed the socket trace, and
every line it emits is `diag=true|is_evidence=false`. It performs **no server
operation and no Wi-Fi change** (neither may stand in for B), and it does not by
itself produce any AUTH, observation or verdict.

### Interface

* Core (NAPI, synchronous, JSON string in/out): `n2_stimulus(kind, id)` with
  `kind ∈ {M, S, R, I, Wb, Ws, D}` and `id` = the stimulus id (1..64 chars of
  `[A-Za-z0-9_-]`, the same rule as the positive-control marker ids). Answer:
  `{ok,result,reason,target_fd,remote,line}` where `result` is `applied`,
  `rejected` or `no-target` and `ok` is true only for `applied`.
* Shell entry: `NetBirdVpnExtensionAbility.pollN2Stimulus()` — the analogue of
  `NetBirdN2Control.sendN2Control`. It returns immediately (not even a file
  probe) unless `this.n2Evidence !== undefined`, reads exactly one request,
  calls the core once, logs the shell receipt and **deletes the request file in
  `finally`** (one launch = one stimulus; the 5 s watcher tick can never replay
  it). The pure parse/format half is `NetBirdN2Stimulus.ets` (zero imports).

### How the operator triggers it

The existing non-secret `n2h_arm_options` private channel, extended with one
more want parameter. No new permission, no new device command family:

```bash
aa start -b cn.alfadb.netbird -a EntryAbility \
  --ps n2h_arm_options '<the same arm ticket that armed this session>' \
  --ps n2_stimulus '{"kind":"M","id":"b-M-1"}'
```

`EntryAbility` writes the payload verbatim to `<filesDir>/netbird-n2-stimulus`
(same filesDir mechanism and basename discipline as the arm ticket); a launch
**without** `n2_stimulus` deletes a leftover request, exactly like the arm
ticket's default-no-arm rule, so a stale stimulus cannot fire later. The
already-running, already-armed extension picks the file up on its next status
watcher tick (≤ `STATUS_POLL_MS` = 5 s) and consumes it. N2 T7a item 7 changes
the ARM TICKET rule this paragraph used to depend on: a launch that carries an
N2 request parameter (`n2_stimulus` / `n2_control` / `n2_a_event`) but **no**
`n2h_arm_options` now **KEEPS** the existing ticket (one
`N2H_ARM_TICKET_KEPT|file=netbird-n2h-arm|note=request-launch-without-arm-ticket`
line) instead of deleting it, so the documented「先 arm，再逐条发请求」flow can
no longer silently disarm a later extension restart. A launch with **no** N2
request parameter keeps the original default-no-arm deletion rule (a stale
ticket still cannot survive an unrelated launch), and an explicit ticket is
always written verbatim. Three repetitions = the same command three times with a
fresh `id` each time. The trigger needs the extension instance that is already
armed; a force-stop or a new VPN session requires a fresh arm first.

### Log lines

* `N2_STIMULUS|id=…|kind=…|target_fd=…|remote=…|mono_ms=…|result=…|reason=…|diag=true|is_evidence=false`
  — one line per call, whatever the outcome. `target_fd`/`remote` are the outer
  socket (or selected endpoint) the class last registered, `-1`/`unknown` when
  there is none. `result` is `applied` / `rejected` / `no-target`; `reason`
  names the exact branch (`shutdown-socket`, `endpoint-drop`, `dns-requery`,
  `not-armed`, `invalid-kind`, `invalid-id`, `no-outer-socket`,
  `shutdown-failed-errno-<n>`, `no-sink`, `no-live-endpoint`).
* `N2_STIMULUS_SHELL|…` — the shell's delivery receipt (request id, kind, id,
  the core's result/reason); the core's line stays authoritative.
* `N2_STIMULUS_DNS|host=…|result=…|detail=…|elapsed_ms=…` — D's forced query.
* `N2_STIMULUS_FILE_WRITTEN|bytes=` / `_WRITE_FAILED|code=` / `_REMOVED|note=` /
  `_REMOVE_FAILED|code=` / `_CLOSE_FAILED|code=` — the EntryAbility write path
  (file name + UTF-8 byte count only; the payload is never logged).
* Correlation with `N2_SOCK`: the registry records a class's outer socket inside
  the **same `first_io` completion that emits that socket's `N2_SOCK` receipt**,
  so `target_fd` is the fd those receipts report, `target_fd` becomes the next
  `N2_SOCK` `dup_fd` after the re-dial, and `remote` is the same endpoint.

### Per-class coverage and limits

| kind | action | covered | NOT covered / limits |
| --- | --- | --- | --- |
| M, S, R | `shutdown(SHUT_RDWR)` on the one outer socket of that class (a private duplicate of the core-owned fd, so a reused fd number can never be hit); the owner takes its existing re-dial path | the **local close/error → re-dial** path, exactly once per socket; only that class is touched; the peer observes the FIN | **no** network-side RST / half-open / blackhole / timeout claim; no AP-side blocking; no server disconnect; one stimulus consumes one registration, so a repeat needs the re-dial's fresh socket |
| I | drop every peer's current ICE session and rearm negotiation with the cooldown cleared (`PeerIceOrchestrator::break_ice`): the session's dup sockets are closed and the next pump re-gathers/re-checks/re-signals | the "endpoint-directed break → re-check" equivalent of the frozen I row | it re-negotiates **all** peers with a live session, not a single host candidate; a transient (gathering) socket is not a target |
| Wb, Ws | drop every peer's currently selected WG path (`reset_peer_paths`: endpoint cleared, the selected egress dup closed, the handshake campaign reset) so the next ICE nomination re-attaches a fresh selected socket and re-handshakes over it | the "endpoint-directed break → re-handshake" equivalent of the frozen W row | **the base device socket (`wg_fwd_open` dup) is deliberately kept** — it has no core-side rebuild path (the shell re-feeds it via `connector_wg_socket_feed`), so a Wb-only peer stays pathless until ICE nominates again; no relay-carrier teardown is implied |
| D | force ONE fresh protected DNS query for the hostname the session last resolved over a protected fd (`force_protected_dns_requery`), on a new protected fd, with its own `N2_STIMULUS_DNS` receipt | the frozen "new query / new fd, response" D stimulus | it does not invent a hostname (`no-target` when the session never resolved one), does not bypass the protected-socket supply, and does not use the system resolver; the alternative "make the next query time out" is **not used** |

`applied` means the syscall returned 0 (or the sink acted on at least one live
endpoint) — never a prediction that a re-dial has already completed. A class
with no registered socket answers `no-target`, and a call outside an armed
session answers `rejected|reason=not-armed` before any syscall.

### Host verification (no device)

* `cargo test --offline --locked` (client/core): unit tests for the registry,
  the action table, the marker shape and the gating; the integration test
  `tests/n2_stimulus.rs` runs the REAL protected dial path on loopback per
  class — per-class isolation against live witness sockets, the peer-observed
  close, a new registration per re-dial, three repetitions, `no-target` and the
  unarmed refusal — plus the sink delegation for I/Wb/Ws/D.
* `node client/entry/selfcheck/run-n2-stimulus.mts`: the pure module as code
  (parse refusals, reply parsing, receipt formatting) and source pins on the
  three-file private channel and on the core surface. The existing loader mocks
  (`exclusion-generation-hooks.mjs`, `relay-exclusion-hooks.mjs`) carry the
  `n2_stimulus` stub and the stimulus module, so the real-extension fixtures
  still link and their suites are unchanged.
* No device measurement was made here. On-device use still requires the
  separately authorized campaign: the AUTH must list this stimulus, its
  per-class limits above and the repetition count, and a network-side
  (AP-blocked) path remains a separate authorization.

## Positive-control private channel (T3 §3, 2026-09-28)

The frozen T3 sandwich (`NetBirdN2Control.ets`) had **no host trigger**: only
the ArkTS export existed, so an authorized operator could not fire a pre/post
control without a rebuilt App. The `n2_control` private channel closes that gap
with the same non-secret want-parameter mechanism as `n2_stimulus`; it is still
diagnostic code (`diag=true|is_evidence=false`) and creates no authorization.

### Interface

* Shell entry: `NetBirdVpnExtensionAbility.pollN2Control()` — reads exactly one
  request, calls the existing `sendN2PositiveControl(target, proto, id)`
  (`sendN2Control` → the production opener `mgmtSocketOpen` for `tcp` /
  `iceSocketOpen` for `udp`, no protect, no net-handle bind), logs the shell
  receipt and **deletes the request file in `finally`** (one launch = one
  control; the 5 s watcher tick can never replay it). The pure parse/format half
  is `NetBirdN2ControlRequest.ets` (zero imports).
* Payload: `{"target":"<ip:port>","proto":"udp|tcp","id":"<marker id>"}`.
  `target` must be one frozen outer destination **inside the armed ticket's
  `capture_32`**; the shape is checked in the shell, the capture membership and
  the core's own rules answer `target-not-captured` / `invalid-control` /
  `duplicate-control` / `control-send-failed` and are reported verbatim.
  `id` uses the same `[A-Za-z0-9_-]{1,64}` marker rule; the core refuses a
  REUSED id, so every sandwich leg needs a fresh one.
* **Batching is NOT implemented**: a `{"controls":[...]}` payload is refused by
  name (`batch-not-implemented`) — three separate launches are the supported
  shape.
* One launch carries at most ONE pending request: with **both** `n2_stimulus`
  and `n2_control` present `EntryAbility` writes NEITHER (one
  `N2_CONTROL_COMBO_REJECTED|stimulus=…|control=…|note=one-pending-request-per-launch`
  line) and removes both leftovers; a launch with only one of them removes the
  other's leftover. N2 P4 extends the SAME rule to a third channel
  (`n2_a_event`): any combination of two or three writes none of them, and a
  refusal involving the A-event channel extends the line with `|a_event=` (see
  「A-column production-event trigger (P4)」). If two SEPARATE launches leave
  both files pending, the
  watcher tick submits the **control before the stimulus** (fixed by call order
  in the tick body), so the "pre-control → event → post-control" three-call
  shape is unambiguous.
* Unarmed: a pending request is still consumed (this is the one deliberate
  difference from the stimulus, which never probes the file when unarmed) and
  answered `result=rejected|reason=not-armed` with **no** core call — so a
  leftover can neither fire later nor be silently dropped.

### How the operator triggers it

```bash
aa start -b cn.alfadb.netbird -a EntryAbility \
  --ps n2h_arm_options '<the same arm ticket that armed this session>' \
  --ps n2_control '{"target":"119.29.29.119:53","proto":"udp","id":"evt1-pre"}'
```

`EntryAbility` writes the payload verbatim to `<filesDir>/netbird-n2-control`
(UTF-8 byte count only, never the payload), and a launch without the parameter
deletes a leftover. The already-running, already-armed extension picks the file
up on its next status watcher tick (≤ `STATUS_POLL_MS` = 5 s) and consumes it.
N2 T7a item 7: a launch that carries `n2_control` but not `n2h_arm_options`
KEEPS the existing arm ticket (`N2H_ARM_TICKET_KEPT`), so the operator no longer
has to repeat the ticket on every request launch; a launch with no N2 request
parameter still clears a leftover ticket. The sandwich itself stays three
separate launches with a fresh `id` per leg (the production event happens
between the pre- and the post-control), and `withN2Sandwich` remains the
explicit in-process convention.

### Log lines

* `N2_CONTROL_SHELL|requestId=…|id=…|target=…|proto=…|result=sent|rejected|reason=…|diag=true|is_evidence=false`
  — one line per consumed request. `result=sent` means the send call returned
  without throwing; `rejected` carries the exact branch
  (`not-armed`, `target-not-captured`, `invalid-marker`,
  `n2-control-open-failed`, `control-send-failed`, …). The CORE's own
  `N2_CONTROL_SENT` / `N2_CONTROL_HIT` / `N2_CONTROL_MISS` / `N2_TUN_LEAK`
  lines stay authoritative for what the marker actually did.
* `N2_CONTROL_SHELL|…|id=none|target=none|proto=none|result=rejected|reason=…` —
  a payload that never became a request (`empty-request`, `invalid-json`,
  `invalid-request`, `batch-not-implemented`, `invalid-target`,
  `invalid-protocol`, `invalid-id`, `request-too-long`).
* `N2_CONTROL_FILE_WRITTEN|bytes=` / `_WRITE_FAILED|code=` / `_REMOVED|note=` /
  `_REMOVE_FAILED|code=` / `_CLOSE_FAILED|code=` / `N2_CONTROL_COMBO_REJECTED|…`
  — the EntryAbility write path (file name + UTF-8 byte count only).
* `N2_CONTROL_READ_FAILED|code=` / `N2_CONTROL_CONSUME_FAILED|code=` — the
  extension's read/consume path (codes only).

### Host verification (no device)

* `node client/entry/selfcheck/run-n2-control.mts` — the pure module as code
  (accepted udp/tcp, every refusal by name incl. `batch-not-implemented`, the
  two receipt formatters, pipeline-character cleaning) plus source pins on the
  three-file private channel, the loader mocks and the `N2_DNS` core surface.
* `run-exclusion-generation.mts` N group — the REAL `EntryAbility` write /
  leftover-removal / combo refusal through the path-aware fake fs, and the REAL
  armed `pollN2Control`: one send call with the parsed request, `result=sent`,
  the unarmed `result=rejected|reason=not-armed` with ZERO core calls, an armed
  send refusal, a malformed request, and the tick ordering
  (control consumed before stimulus in one tick).
* The two loader mocks (`exclusion-generation-hooks.mjs`,
  `relay-exclusion-hooks.mjs`) carry the recording `n2_control` sender stub and
  the staged `NetBirdN2ControlRequest` shim, so every real-extension fixture
  still links.

## D-row same-ID response receipt (`N2_DNS`, §4 D, 2026-09-28)

Frozen §4 D uses the protected DNS fd's sent query and same-ID response as the
substitute oracle when no independent resolver log exists. The production path
(`client/core/src/protected_dns.rs`, all callers: management/signal resolution,
relay, ICE, and the D stimulus re-query) now emits one receipt per production
query while the `n2_evidence` arm holds the socket trace — **zero output while
unarmed**:

```
N2_DNS|query_id=…|qname=…|server=…|fd=…|dup_fd=…|sent_mono_ms=…|response=matched|mismatched|timeout|error|rcode=…|answer_count=…|elapsed_ms=…|diag=true|is_evidence=false
```

* `query_id` is the transaction id actually sent; `qname` is the queried
  hostname (a name, never the answer); `server` is the resolver `IP:port` the
  query went to.
* `fd` / `dup_fd` follow the `N2_SOCK` convention: the shell original the armed
  protect receipt registered for this descriptor and the core-owned descriptor.
  If the armed trace has no registration for the descriptor, `fd` falls back to
  the same value as `dup_fd` (no invented original).
* `sent_mono_ms` is the monotonic time the send call returned; `elapsed_ms` is
  the whole per-server query box.
* `response` is the query-id vs response-id verdict: `matched` (a response
  arrived, its id equals the sent id and the A-record validation passed),
  `mismatched` (a datagram arrived whose transaction id differs),
  `timeout` (no datagram within the query timeout) or `error` (send/recv
  failure, or a same-id response the parser rejected).
* `rcode` is the response header RCODE and `answer_count` its ANCOUNT — both
  `-1` when no response body was decoded. **Answer content is never logged.**
* `rcode`/`answer_count`/`response` describe this App's own protected fd only;
  they are a diagnostic receipt, not the authoritative byte and not a pass on
  their own. Pair it with the frozen §3 evidence (device fd chain, sandwich
  controls, campaign-wide TUN zero-hit) as §4 D requires.

Host verification: unit tests in `client/core/src/n2_socket_trace.rs` (the
receipt is armed-only — `None`, i.e. zero output, when unarmed — and carries
every documented field for the matched / mismatched / timeout verdicts, plus
the registered shell original) and in `client/core/src/protected_dns.rs` (the
four verdicts, and the id/RCODE/ANCOUNT header decoding including the truncated
cases). No device measurement was made here.

## A-column production-event trigger (P4, 2026-09-28)

Preregistration v4.1 §4「A 列生产事件的操作性定义」/ 实现前置项 **P4**. This is
the **default-off, evidence-switch-gated** trigger primitive for the A column:
inert and unreachable until an accepted `n2_evidence` ticket armed the socket
trace, and every line it emits is `diag=true|is_evidence=false`. It performs no
server-side state change, opens no new connection and creates no new `Sync`
stream.

### Interface

* Core (NAPI, synchronous, JSON string in/out): `n2_a_event(kind, id)` with
  `kind ∈ {M, R, D, S}` and `id` = the same `[A-Za-z0-9_-]{1,64}` marker rule
  as the stimulus/control ids. Answer:
  `{ok,result,reason,kind,fd,dup_fd,line}`; `ok` is true only for
  `sent`/`armed`/`requery`/`observed`.
* Shell entry: `NetBirdVpnExtensionAbility.pollN2AEvent()` — the analogue of
  `pollN2Control`. It reads exactly one request, calls the core once, logs the
  shell receipt and **deletes the request file in `finally`** (one launch = one
  trigger). The pure parse/format half is `NetBirdN2AEvent.ets` (zero imports).

### Per-kind contract (frozen v4.1 §4, unchanged criteria)

| kind | action | emitted |
| --- | --- | --- |
| **M** | ONE read-only idempotent real `GetServerKey` (`grpc.rs:430`) on the **EXISTING** logged-in management client — the `logout_slot` clone whose tonic `Channel` is the same connection the Sync stream runs over. **No new dial, no second `Sync` stream.** | `phase=begin|result=begin`, then `phase=end|result=sent` / `phase=end|result=error|reason=management-rpc-<class>` (sanitized class only, never server text) |
| **R** | NO active send: arm a watcher for the next **server-driven** `HealthCheck` and record the production echo (`relay_client.rs:2117-2120`). Wait bound `R_WAIT_MS = 90_000` ms on the session runtime. | `phase=begin|result=armed|reason=healthcheck-watch|detail=wait_ms=90000`, then `phase=end|result=observed|reason=server-healthcheck-answered` or `phase=end|result=timeout|reason=healthcheck-wait-exceeded` |
| **D** | Q1: the first A event of the session calls the existing requery primitive ONCE (`force_protected_dns_requery`, one fresh protected fd). Any prior `n2_stimulus` application or prior A event refuses. | `phase=begin` then `phase=end|result=requery|detail=receipt-in-N2_DNS` / `phase=end|result=rejected|reason=prior-stimulus` |
| **S** | **NO active send** (N2 T7a item 5, frozen §4 S row (i)): arm a watcher for the NEXT **natural** silence-triggered `HEARTBEAT` (`Body.type = 6`) the registered receive watchdog sends anyway (after `RECEIVE_PROBE_AFTER` = 20 s of stream silence), on the SAME signal stream/connection. Wait bound `S_WAIT_MS = 45_000` ms on the session runtime. `fd`/`dup_fd` are the live `S` pair the `N2_SOCK` receipts report, taken from the armed trace. | `phase=begin|result=armed|reason=signal-heartbeat-watch|detail=wait_ms=45000`, then `phase=end|result=observed|reason=watchdog-heartbeat-sent|detail=natural-heartbeat` or `phase=end|result=timeout|reason=heartbeat-wait-exceeded` |

Unarmed: `phase=end|result=rejected|reason=not-armed` before any RPC/syscall.
Unknown kind → `invalid-kind`; malformed id → `invalid-id`. An accepted request
emits its `phase=begin` line when its trigger starts (M/D just before the call,
R/S as soon as the watch arm returned) and its `phase=end` line when the outcome
is known; a request refused by a gate emits the single `phase=end` line, because
no trigger ever began. The begin LINE and the event WINDOW are separate facts
for S: its arm emits the line without opening a window (P1-6, below).

### Log line

```
N2_A_EVENT|phase=begin|end|id=…|kind=M|R|D|S|fd=…|dup_fd=…|remote=…|result=…|reason=…|detail=…|event_id=…|mono_ms=…|diag=true|is_evidence=false
```

Both phases carry the identical field set and order. `id` and `event_id` are
the same request id (`event_id` is the join name the forced write receipt uses).
`fd` is the shell ORIGINAL and `dup_fd` the core-owned descriptor of the live
outer socket — the **same pair the `N2_SOCK` receipts report**, taken from the
armed trace itself (`n2_socket_trace::live_of`), so an A event can never be
attributed to a socket no `N2_SOCK` line describes. D claims no fd here: its
authoritative device record is the `N2_DNS` receipt the requery emits.

**Event window (T7a item 4).** For the immediate kinds (M/D) `phase=begin` opens
an event window on that `dup_fd`; the matching `phase=end` closes it. EVERY write
completed on that descriptor inside the window is receipted **unconditionally** —
the post-connect receipt throttle is bypassed — with `event_id=<id>` on the
`N2_SOCK|phase=subsequent_io|…` line (see「Outer-socket lifecycle receipts」).
The one production write an event causes therefore always has a receipt, even
when it is not the first / every-128th / 30-s write of its descriptor. For R the
window opens with the arm — the server may send its `HealthCheck` at any moment,
so only an already-open window can contain the response write. For S the arm
opens NO window (N2 T7a round 3, P1-6): its window exists only around the one
natural heartbeat write, see below.

### How the operator triggers it

```bash
aa start -b cn.alfadb.netbird -a EntryAbility \
  --ps n2h_arm_options '<the same arm ticket that armed this session>' \
  --ps n2_a_event '{"kind":"M","id":"a-M-1"}'
```

`EntryAbility` writes the payload verbatim to `<filesDir>/netbird-n2-a-event`
(UTF-8 byte count only, never the payload). A launch without the parameter
deletes a leftover. The **one-pending-request-per-launch** combination rule now
spans THREE channels (`n2_stimulus`, `n2_control`, `n2_a_event`): a launch
carrying more than one writes NONE of them and removes every leftover — the
pre-existing `stimulus`+`control` refusal keeps its exact line, and a refusal
involving the A-event channel extends it with `|a_event=`. `N2_A_EVENT_SHELL|…`
is the shell delivery receipt; the core's line stays authoritative.

### Host verification (no device)

* `cargo test --offline --locked` (client/core): unit tests in
  `client/core/src/n2_a_event.rs` (gating, the S refusal, the D Q1 gate, the R
  watch completion/timeout, one-trigger-one-line) and the integration test
  `tests/n2_a_event.rs::m_runs_one_real_rpc_on_the_existing_connection_only`,
  which runs the REAL protected dial path against an in-process tonic server
  and asserts: exactly ONE accepted connection across the whole test, the
  `N2_A_EVENT|kind=M` fd/dup pair byte-identical to the `N2_SOCK` pair, and the
  live registration unchanged after two `GetServerKey` calls.

## I/Ws sandwich orchestration (P5, 2026-09-28)

Preregistration v4.1 §4 A 列 I/Ws 行 + §3 第 2 条 / 实现前置项 **P5**. Armed
only; the shell inserts the control markers at wait points AFTER the TUN tee
attach and **never changes the ICE/WG send content or order**.

* **Core state machine** `client/core/src/n2_sandwich.rs` owns the ordering
  verdict: `attach_anchor` is called at the real tee attach
  (`wg_device.rs` `try_build`, right after the `before-tun-attach` gap), the
  first successful I/Ws send is observed at the single `n2_socket_trace::first_io`
  call site, and the pre control's HIT is recorded by the armed tee itself
  (`N2DestinationTee::consume_control` → `n2_sandwich::control_hit`).
* **Shell orchestration** in `settleCreate`: `n2IceSandwichPre()` runs between
  the WG data-plane feed (the attach) and `protectAndFeedIceSocket()` — the
  frozen「首轮 ICE 池馈入之前」; `n2WsSandwichPre()` runs right after the pool
  feed and therefore before any ICE nomination can select the egress socket.
  Each sends its PRE control through `sendAndWaitN2Control`
  (`NetBirdN2Control.ets`) and waits for the HIT. The POST controls are sent by
  the watcher tick (`pollN2Sandwich`) once the core has observed the first real
  production send.
* **Wait bounds** (written down, not tunable at the call site): the pre-HIT
  wait before the first send is the core control window `PRE_WAIT_MS = 3_000`
  ms; the first-send wait before the post is `POST_WAIT_MS = 90_000` ms. A leg
  that never sees its send is closed as `result=timeout`.
* **`Ws` keeps the frozen order**: `peer_conn.rs`'s "attach the egress socket
  FIRST, land the endpoint only then" (`:1230-1239`) is untouched — the shell
  never defers or reorders `apply_endpoint`.
* **UNSAT**: a first I/Ws send observed BEFORE the attach anchor emits
  `result=unsat|reason=first-send-before-attach`, whether or not a leg was ever
  requested; a first send after the attach but before the pre control (or
  before its HIT) is `result=unsat|reason=first-send-before-pre-control` /
  `pre-control-not-before-first-send`. A missed pre control closes the leg as
  `result=unsat|reason=pre-control-miss`.

### Log line

```
N2_SANDWICH|kind=I|Ws|phase=pre|post|first-send|marker=…|event_ref=…|result=…|reason=…|fd=…|dup_fd=…|remote=…|wait_ms=…|mono_ms=…|diag=true|is_evidence=false
```

**The pre and post controls carry DIFFERENT markers (N2 T7a item 1).** The shell
derives both from the instance request id (`n2s-<kind>-<id>-pre` /
`n2s-<kind>-<id>-post`) and registers both with
`n2_sandwich_pre(kind, pre_marker, post_marker)`; the core hands the POST
marker back through `n2_sandwich_poll()` and every `phase=post` line records
THAT marker (`marker=` is the marker of its own phase), so the post control can
never be refused as a reused id (`duplicate-control`) the way reusing the pre
marker would force. `event_ref` is the observation the phase is anchored to
(`attach@<mono_ms>` / `first-send@<mono_ms>` / `pre@<mono_ms>` / `none`).

`N2_SANDWICH_SHELL|…` is the shell's delivery receipt. Unarmed, every entry
point is a **silent no-op** (no state, no line, no control).

### Interface (NAPI)

* `n2_sandwich_pre(kind, pre_marker, post_marker)` → `{ok,result,reason,kind,marker}`;
  the shell calls it BEFORE sending the pre control and skips the send when
  `ok` is false (the core already logged the exact `unsat`/`rejected` reason).
  A missing/reused post marker is refused by name (`invalid-post-marker`).
* `n2_sandwich_poll()` → `{ok,posts:[{kind,marker}]}`; one entry per leg whose
  first production send was observed after the pre HIT, where `marker` is the
  POST control's own marker the shell must send. Also closes timed-out legs.
* `n2_sandwich_post(kind, marker, result, reason)` → closes one leg with the
  shell's post verdict (`observed` = the post marker HIT, `unsat` = it MISSed);
  `marker` is the POST control's marker (the leg is located by it).

### Host verification (no device)

* `cargo test --offline --locked` (client/core): unit tests in
  `client/core/src/n2_sandwich.rs` (attach-anchor gating, pre→HIT→send
  ordering, the frozen `first-send-before-attach` token, the pre-MISS close, the
  `POST_WAIT_MS` timeout, unarmed zero-output) and the integration test
  `tests/n2_a_event.rs::sandwich_pre_hit_precedes_the_first_send_and_unarmed_is_silent`,
  which drives the REAL `N2DestinationTee::consume_control` to mark the pre HIT
  and then the first send.
* `node client/entry/selfcheck/run-n2-a-event.mts` — both new pure modules as
  code (parsers, target selection, marker rule, poll parsing, every formatter)
  plus source pins on the extension / EntryAbility / core wiring.
* `run-exclusion-generation.mts` N group — the REAL `EntryAbility` three-way
  combination rule through the path-aware fake fs and the REAL armed
  `pollN2AEvent` / `pollN2Sandwich` against the recording core seams.

No device measurement was made here. On-device use still requires the
separately authorized campaign and the T7a conformance review the frozen record
demands.

## Peer-side outer-UDP source receipt (P7, 2026-09-28)

Implementation prerequisite **P7** of frozen v4.1. The Q2 ruling admits the peer
process's own `recvfrom` view as **辅助证据（应用层，非独立网络侧 oracle）**:
it may strengthen the cellular `Wb` negative evidence and the peer-side receipt
of `Ws`, and it **never replaces host-side capture and never fills a cell whose
capture is missing** (v4.1 §4 Wb/Ws substitute-oracle rules are unchanged —
this section adds no oracle).

### Interface

* Switch: `nbinterop --outer-receipt` (**host-only, default OFF**). Without it
  every seam is one `armed()` atomic load that returns before reading any clock,
  socket name or peer table, so the pre-P7 output is byte-identical.
* Core: `netbird_core::n2_outer_receipt` (`arm(bool)` / `armed()` /
  `note_udp(local, src, datagram, peer)`), armed by the CLI **before the first
  socket is created**, so the very first outer datagram is recordable. This is
  an operator/CLI switch, not an ArkTS `n2_evidence` ticket: it is a host-side
  diagnostic that changes no device behaviour.
* Seams (every outer UDP receive point of the host path):
  * `wg_device.rs::WgDevice::service_udp` — the WG outer socket
    (`--wg-port` / ephemeral). The peer label is the registered peer whose
    `endpoint` equals the source, as the N5c 8-char public-key short prefix;
    any other source is `peer=unknown` — the off-path case the cellular legs
    need, recorded, never dropped from the log;
  * `ice_session.rs::IceSession::drain_socket` — the ICE socket carrying the
    connectivity checks and, after nomination, the `Ws`/`Wb` WG datagrams
    (`peer=` = the session's owning peer, set by `peer_conn.rs`);
  * `ice.rs::gather_interface` (the `gather_candidates` path) — the one-shot
    STUN gather socket (ephemeral mode only; the fixed `--ice-port` path never
    gathers). STUN servers are not WG peers, so `peer=unknown` there by
    construction.
* Not covered (honest gap): no physical-interface path, no loss before the
  socket, no exclusion of same-host injection, and no datagram the kernel never
  delivered to these sockets.

### Log lines

```
nbinterop: n2h-outer-receipt|first|src=<ip:port>|local=<ip:port>|type=<stun|wg-init|wg-resp|wg-cookie|wg-data|other>|peer=<known peer short prefix|unknown>|mono_ms=…|unix_ms=…
nbinterop: n2h-outer-receipt|summary|src=<ip:port>|packets=…|bytes=…|types=…|local=<ip:port>|peer=…|mono_ms=…|unix_ms=…
```

* One `first` line per **new** `(local, src)` pair — including `peer=unknown`
  sources and any datagram type; then per source one `summary` line every 5 s,
  or immediately when a NEW datagram type appears for that source (`types=`
  lists only the non-zero types, in `stun,wg-init,wg-resp,wg-cookie,wg-data,
  other` order). No per-packet lines.
* `type` is decided by the RFC 5389 §6 magic cookie `0x2112A442` at bytes 4..8
  FIRST, then by the WG message type in the first byte (1–4); everything else
  is `other`.
* `local` is this socket's own bound name (`unknown` only if `getsockname`
  failed); `mono_ms` is the injected monotonic clock the datagram was handled
  on, `unix_ms` the host wall clock of the same receipt.

### Evidence level

`n2h-outer-receipt` lines are **auxiliary application-layer evidence
(`diag`/diagnostic, not an oracle)**: they live on stderr with the other
`nbinterop:` host lines, carry no `is_evidence` promotion of their own, and must
be quoted in the campaign record only alongside (a) the device's own physical
interface send metadata and (b) the host-side capture. A `Wb`/`Ws` cell whose
capture is missing stays missing.

### Host verification (no device)

* `cargo test --offline --locked` (client/core): unit tests in
  `client/core/src/n2_outer_receipt.rs`
  (`classify_uses_the_stun_cookie_first_then_the_wg_message_type`,
  `first_datagram_of_a_new_source_prints_one_line_and_unknown_peers_are_recorded`,
  `summaries_are_throttled_to_five_seconds_or_a_new_type`,
  `a_late_known_peer_attribution_is_reported_on_the_next_summary`,
  `disarmed_note_udp_emits_nothing`), the WG-socket seam test
  `wg_device.rs::feed_tests::outer_receipt_reports_known_and_unknown_sources_and_stays_silent_when_off`,
  and the end-to-end process test
  `tests/outer_receipt_p7.rs::outer_receipt_reports_a_real_outer_udp_source_end_to_end`,
  which sends a REAL loopback UDP datagram to the fed nbinterop WG listening
  port: switch OFF ⇒ the device counts the drop but no line is emitted; switch
  ON ⇒ the `first` line names that source. It lives in its own test binary on
  purpose: `tests/host_interop_n9.rs` carries an fd-NUMBER assertion that any
  concurrently running socket-opening test can steal, so widening that file's
  race was avoided rather than papered over.
* `tests/ice_session_e2e.rs::outer_receipt_reports_the_ice_socket_source_and_stays_silent_when_off`
  — the ICE/STUN socket seam (`--ice-port` feeds exactly this socket): a REAL
  loopback ICE convergence emits the `first` line with `type=stun`, the
  session's `peer=` label and this socket's own `local=`, and the identical
  convergence under the switch OFF emits nothing.

No device measurement was made here.

## Signal receive watchdog and the S A-event (P8, 2026-09-28)

Implementation prerequisite **P8** of frozen v4.1 §4「A 列生产事件的操作性定义」
(`S` row) — and, first of all, a **product** behaviour: the signal receive
watchdog is default-on, independent of `n2_evidence`.

### The watchdog (product)

`SignalSession`'s registered receive loop measures silence from the newest
received frame (registration counts as one) and

* at **20 s** of silence (`RECEIVE_PROBE_AFTER`) sends ONE self-addressed
  `HEARTBEAT` (`SignalClient::send_heartbeat` = upstream `sendReceiveProbe`)
  through the unary `SignalExchange/Send` on the **SAME** pooled
  stream/connection — no new dial, no other message changed;
* treats **any** received frame (the echo included; a malformed frame is
  liveness too) as the restart of that window;
* at **30 s** of silence (`RECEIVE_DEAD_AFTER`) gives the stream up through the
  **existing** `Broken` → backoff → re-register path. The probe-return window
  is therefore `30 − 20 = 10 s`, exactly upstream's
  (`shared/signal/client/grpc.go:29-41` constants, `:522-553` watchdog: probe
  after 30 s silence, reconnect if it does not return within 10 s). Probing ten
  seconds earlier keeps the 30 s silence→reconnect boundary the module already
  documented as the outer edge instead of moving it to 40 s.
* a heartbeat send FAILURE is transport-level by construction (the frame is a
  self-addressed, always well-formed body) and takes the same
  break/reconnect path.

Only the send time of that production frame is controllable:
`signal::request_receive_probe()` (async, one reply per request, answered when
the unary send call returned) is the seam the A-column `S` event uses. The
handle is process-global — the CURRENTLY registered signal stream (one connector
session per process) — and is withdrawn with its registration; with no
registered stream the call answers `Err("no-registered-signal-stream")` and
nothing is sent. The authoritative send receipt stays the socket wrapper's
`N2_SOCK|phase=subsequent_io|kind=S`; the `N2_A_EVENT|kind=S` line only reports
the dispatch plus the live `fd`/`dup_fd` pair from the armed trace.

**The `S` A-event observes the NATURAL heartbeat (N2 T7a item 5, frozen §4 S row
(i)).** `n2_a_event::apply("S", …)` no longer asks the watchdog for a probe: it
arms an observer (`Trigger::arm_s_watch`, box `S_WAIT_MS = 45_000` ms) for the
NEXT silence-triggered `HEARTBEAT` the watchdog sends on its own, and
`signal.rs` calls
`n2_a_event::natural_heartbeat_about_to_send()` /
`natural_heartbeat_sent()` immediately around that `send_heartbeat()`. So only a
frame the production client would have sent anyway can complete the event:
`N2_A_EVENT|kind=S|phase=end|result=observed|reason=watchdog-heartbeat-sent|detail=natural-heartbeat|event_id=<id>`,
or `…|phase=end|result=timeout|reason=heartbeat-wait-exceeded|detail=wait_ms=45000`.

**The `S` arm opens NO event window (N2 T7a round 3, P1-6).** The arm emits
`phase=begin|result=armed|reason=signal-heartbeat-watch|detail=wait_ms=45000` and
nothing else. Before this fix the window was opened at arm time and therefore
stood open for the whole 45 s box: EVERY write on the signal socket in that box
— an outgoing control frame, a reconnect handshake, any other S write — was
forced into a receipt carrying this event's `event_id`, so a judge could not
bind a forced receipt to the HEARTBEAT the event is about. Now the window is
opened by `natural_heartbeat_about_to_send()` (immediately before the
`send_heartbeat()` that is the event) and closed by `natural_heartbeat_sent()`
(it emits the `phase=end|result=observed` line) — both in `src/signal.rs`,
`Step::Silence if !probed`. A FAILED heartbeat write closes the window through
`natural_heartbeat_not_sent()` without completing the watch, so a broken attempt
leaves neither an open window on a soon-to-be-reused descriptor number nor an
`observed` claim. Consequence for the executor/judge: an
`N2_SOCK|phase=subsequent_io|kind=S|…|event_id=<id>` receipt now means the
HEARTBEAT write of that event, and the absence of `event_id` on other `kind=S`
receipts inside the same box is expected, not a missing receipt.
`signal::request_receive_probe()` is kept as a **product-internal** seam (the
watchdog's own probe capability) and is no longer called by any N2 path.

### Host verification (no device)

* `cargo test --offline --locked` (client/core):
  * `tests/signal_session.rs` — the product behaviour against the real mock
    server: `watchdog_thresholds_are_the_documented_pair`,
    `watchdog_probes_after_silence_and_the_echo_keeps_the_stream_up`,
    `a_received_frame_restarts_the_watchdog_silence_window`,
    `watchdog_reconnects_when_the_probe_is_never_echoed` (the echo knob off ⇒
    `Broken` naming the watchdog, then a fresh registration on the same
    connection).
  * `tests/signal_watchdog.rs` (own binary: the probe handle is a process
    singleton) — `an_external_probe_request_sends_one_heartbeat_immediately`
    (the product-internal seam: one request ⇒ one self-addressed probe, same
    connection; no stream frame; unregistered refuses) and
    `s_a_event_observes_the_natural_watchdog_heartbeat_on_the_existing_stream`
    (N2 T7a item 5: the `S` event only ARMS the observer — `result=armed`,
    the `fd`/`dup_fd` pair byte-identical to `N2_SOCK`, and NO event window yet
    (round 3, P1-6) — then the NATURAL silence-triggered heartbeat completes it
    as `phase=end|result=observed|event_id=…` and closes the window, ONE
    accepted connection, the decrypted frame is `Body.type = 6`, no
    OFFER/ANSWER/CANDIDATE created; a refused arm is `result=error` and sends
    nothing).
  * `n2_a_event.rs` unit tests
    (`kind_s_needs_a_live_signal_socket_and_observes_the_natural_heartbeat`,
    round 3: the arm leaves the window closed and only the heartbeat hook opens
    it; `s_window_covers_the_heartbeat_write_and_closes_when_that_write_fails`)
    and `tests/n2_a_event.rs::s_event_arms_the_natural_heartbeat_watch_and_refuses_without_a_live_socket`.
  * `client/entry/selfcheck/run-n2-a-event.mts` — pins the `S` observer
    contract (including「the arm opens no window」), the
    `natural_heartbeat_not_sent` failure hook and the 20 s/30 s watchdog
    constants on the shipped sources.
* On-device use of `n2_a_event kind=S` still requires the separately authorized
  campaign; the frozen `S(A)` row is what it feeds (the natural heartbeat,
  observed rather than triggered).

## T7a conformance fixes (2026-09-28)

The product-side diagnostic issues the T7a independent review named are fixed
here; every change is arm-gated diagnostic output (except item 7, which is an
EntryAbility launch-time ticket rule). The **exact** field list, order, value
sets and units of every line below, with the source function that formats it,
is [`n2-log-formats-20260928.md`](n2-log-formats-20260928.md); its executable
half is `client/core/tests/n2_log_samples.rs` +
`client/core/tests/fixtures/n2-log-samples.txt` (byte-for-byte assertion).

1. **Sandwich post control marker** — the post leg has its OWN marker
   (`n2_sandwich_pre(kind, pre, post)`), the poll hands it back, and every
   `phase=post` line records it (see「I/Ws sandwich orchestration (P5)」).
   Before this fix the shell sent the post control with the PRE marker, which
   the control registry refuses as `duplicate-control`.
2. **Control requests are findable by request id** — `id=` and `marker=` on
   `N2_CONTROL_HIT`/`_MISS`/`_SENT` (see「T3 positive controls and sandwich」).
3. **TUN tee continuity** — `N2_TUN_TEE_STATS` (frames/decode_fail/dropped) on
   the session cadence, and `N2_TUN_GAP|from_mono=|to_mono=` for tee
   mount / TUN replacement / tee detach (see the T3 paragraph above).
4. **A-event begin/end + forced receipts** — `phase=begin` before the trigger,
   `phase=end` with the outcome, and an unconditional
   `N2_SOCK|phase=subsequent_io|…|event_id=<id>` for every write inside the
   window (see「A-column production-event trigger (P4)」and「Outer-socket
   lifecycle receipts」). The S window holds exactly the natural heartbeat
   write, not the 45 s box (round 3, P1-6).
5. **`S` observes the natural watchdog heartbeat** (45 s box) instead of asking
   for an immediate probe (see「Signal receive watchdog and the S A-event
   (P8)」).
6. **Field completion** — `N7_WG_FEED|adopted` carries
   `endpoints=`/`egress_replayed=`/`mono_ms=`; `VPN_WG_DATAPLANE_FED` carries
   `mono_ms=` (from the core clock, alignable with that same call's
   `N7_WG_FEED|adopted`) and `wall_ms=`; `phase=protect_settle` carries
   `status=`/`protect_generation=`/`local=`; `N2_FD_SEND` is unchanged
   (`packets`/`bytes`/`mono_ms`).
7. **Arm ticket protection** — a launch that carries an N2 request parameter
   (`n2_control`/`n2_stimulus`/`n2_a_event`) but no `n2h_arm_options` KEEPS the
   existing ticket (`N2H_ARM_TICKET_KEPT`) instead of deleting it; a launch with
   no N2 request keeps the original default-no-arm deletion rule (see the
   operator-trigger sections above). Self-checked through the REAL EntryAbility
   in `run-exclusion-generation.mts` (`N3b`).
8. **Format baseline** — the doc + the generated sample file + the test that
   keeps them identical.

## fd-reuse evidence integrity (2026-09-28)

Two registration-table gaps in the outer-socket receipts are closed: the
pre-device WG-egress dup probe now drops its trace registration BEFORE closing
the probe number (`probe_fd_dupable`'s discipline), and a FAILED protected
management dial completes from the registration captured while the descriptor
was still valid (`n2_socket_trace::detach`/`dial_failed`) instead of looking the
already-released number up again — a descriptor number reused in either window
can no longer inherit a stale `kind`/`request_id`/`original_fd`, nor lose its
own registration to another socket's failed receipt. Negative-tested by
`wg_device::feed_tests::pre_device_egress_probe_leaves_no_registration_behind`
and `mgmtsock::tests::failed_dial_completion_reads_only_its_own_detached_registration`.

## C/D App-side switching sandwich + app-internal evidence ring (2026-09-29)

The first batch's C column (Wi-Fi → cellular) and D column (cellular → Wi-Fi)
have a structural problem the A/B columns do not: during the cellular phase the
host has **no live channel at all** — the `hdc` link is on Wi-Fi, and the HiLog
buffer can rotate or flow-control, dropping `N2_*` evidence lines (or the leak
warning itself). Frozen §3, however, requires a pre control, the production
packet and a post control for **every** production event, with both HITs in the
vpn-tun from-OS tee. The user's decision is implemented here: the App keeps its
own verbatim copy of the logs and fires the controls itself; the host issues one
plan while it still has a live link and fetches everything after returning to
Wi-Fi.

Full design, basis, operator runbook and what a re-review still has to settle:
[`n2-cd-app-sandwich-20260929.md`](n2-cd-app-sandwich-20260929.md).
Line formats: [`n2-log-formats-20260928.md`](n2-log-formats-20260928.md) §8–§10.

* **Plan channel** `n2_cd_plan` — the fifth want parameter
  (`--ps n2_cd_plan '{"column":"C"|"D","kind":"M|S|R|D|I|Wb|Ws","rep":n,"id":"…"}'`),
  same filesDir private channel and same one-pending-request-per-launch rule as
  `n2_control`/`n2_stimulus`/`n2_a_event` (a launch carrying more than one of
  the SIX writes none; a combination the three legacy branches do not cover
  emits `N2_CONTROL_COMBO_REJECTED|pending=<file,…>`). Default off: unarmed is a
  silent no-op.
* **Acceptance only REGISTERS the plan.** Both columns are accepted while the
  App is on Wi-Fi (`net_before` must be `wifi`, else
  `unsat|start-bearer-not-wifi`); `C` is then in position, `D` is PRE-SET
  (`reason=await-start-bearer`) and reaches its cellular start through its own
  **preparation hop**: the Wi-Fi → cellular transition is recorded with
  `n2_cd_prepare` (`phase=prepare|result=prepared|reason=not-a-d-measurement` —
  no control, no recovery gate, NOT the D measurement), and only the
  cellular-side `VPN_NETWORK_RECOVERED` hands the core `n2_cd_ready`
  (`phase=prepare|result=ready`). No control is sent at acceptance: a control
  sent then would sit on the OLD network and the core refuses it
  `pre-before-switch`.
* **The FORMAL switch carries the identity baseline.** `n2_cd_identity(kind)`
  (read-only, `{ok,fd,dup_fd,generation,net_id}`, no live registration and
  `ok=false` + `no-live-registration` when the armed trace has none) supplies
  the OLD network's `(fd, dup_fd, protect_generation)`; the shell reports it
  with `n2_cd_switch_identified`. A missing/negative field is passed as `-1` and
  the core fail-closes by name (`switch-identity-required` /
  `switch-identity-missing` / `switch-net-id-unmeasured` /
  `switch-generation-unmeasured`); D additionally requires the minimum dwell
  `DWELL_MIN_MS = 3_000` on the prepared bearer.
* **The pre control goes out on the NEW network, and it gates the re-feed.**
  Once the switch is `switched`, the core's own poll asks for it (`step=pre`);
  the shell sends `<id>-pre` through the kind's own production opener and only a
  tee-observed HIT (`step=recover`) lets `recoverPhysicalNetwork()` run. So the
  pre is on the SAME current network as the event and precedes the re-feed.
* **The receipt is a NEW identity, not a changed counter.** `receipt` requires
  the `protect_generation` to differ from the switch baseline AND the
  `(fd, dup_fd)` pair to differ, `net_id` to equal the switch measurement,
  `protect` success, the I/O instant to be LATER than the pre HIT, and for
  `M`/`S`/`R` a real `phase=subsequent_io` business write (a completed connect is
  `connect-only`). The process-global registration counter is no longer part of
  any verdict.
* **Wb is NOT exempt from the controls any more.** The base socket has no
  production write, so its receipt is the NEW generation's
  `phase=protect_settle|status=success` (`wb-protect-settle-missing` otherwise);
  it is fed by the CORE TAKEOVER of the validated dup
  (`n2_socket_trace::adopted`, called from `wg_device.rs`'s
  `adopt`/`reattach_socket`), bound to the taken-over `dup_fd` and to that
  registration's OWN protect-settle instant (never the takeover clock), and a
  failed protect / dup / feed / adopt produces no `exempted`; the plan still
  sends BOTH controls (frozen:L39 requires ≥3 attributable positive controls per
  cell, and v4.1's exemption covers only the "real production send" criterion).
* **Direction judgement** — the pure module classifies the measured bearer
  (`bearerTypes` + link addresses); column C requires Wi-Fi → cellular and D the
  reverse. A mismatch is `unsat|direction-mismatch`, an unmeasurable bearer is
  `unsat|bearer-unknown` (never a guess). The carrier facts are read through the
  SAME handle `physicalDefaultNetId()` selected (`physicalDefaultBearerFacts`,
  B5), so the direction and the switch judgement can never disagree about which
  network is being measured.
* **Ring file** — an armed-only app-internal ring in the App's own sandbox:
  every core `N2_*`/`N7_WG_FEED`/`N2_TUN_*` line (the tee lives in
  `hilog::emit`) plus the shell-side lines of those families the extension
  forwards (one string, logged and forwarded together), including
  `VPN_NETWORK_BASELINE` / `VPN_NETWORK_RECOVERED` / `VPN_NETWORK_RECOVERY_FAIL`.
  Each persisted record carries a **durable** `seq` and the session token
  (`N2R|sess=…|seq=…|mono_ms=…|wall_ms=…|line=…`), so a page is addressed by seq
  and not by a line index over concatenated segments. Capped at 4 MiB × 3
  segments; rotation is ACK-gated (an un-retrieved oldest segment is never
  deleted), records the dropped seq span in `N2_RING_ROTATED`, reports what was
  NOT persisted as `N2_RING_LOST`/`N2_RING_OVERFLOW` with their seq range, and
  the head/tail/gap inventory is one `N2_RING_MANIFEST` line. The ring never
  lets a HiLog failure or throttle change what it records.
* **Teardown order** — `onDestroy` runs `connectorStop()` → `destroyOnce()` and
  only THEN `n2_ring_status()` + `n2_ring_disable()`, so the closing lines (the
  TUN gap and the descriptors the destroy closes) still enter the ring; a
  `N2_RING_STATS` line is also emitted every 60 s while armed, by the core.
* **Fetch** — the vendor-documented bundle transfer
  (`hdc file recv -b cn.alfadb.netbird ./data/storage/el2/base/files/<file> …`,
  debug-signed + launched App; `E003001`/`E005004`/`E005102`), with the
  `n2_ring_dump` private channel as the fallback. The fallback is
  **seq-paged**: the shell asks for `n2_ring_manifest()` first, forwards the
  request document verbatim (`{"from_seq":…,"to_seq":…,"count":…,"session":…}`;
  a legacy `{"start":…}` document is refused
  `legacy-start-index-unsupported` by BOTH sides), re-emits the core's
  `N2_RING_CHUNK` records and one `N2_RING_DUMP` summary **on HiLog only** (the
  chunk records duplicate ring content and the summary is not ring content at
  all). **A dump never advances the ACK**: its `N2_RING_DUMP_SHELL` receipt
  reports the core's watermark as `ack_watermark=` and calls no ACK; only the
  shell's own receipt is written back into the ring. The retrieved watermark
  moves ONLY through the **explicit host ACK channel** (`n2_ring_ack`, a
  SEPARATE sixth want parameter with the document
  `{"session":"<the arm token the records carry>","up_to_seq":<durable seq>}`),
  which the host may send only after it has PERSISTED the fetched bytes and
  verified the window is complete; the core binds it to the CURRENT arm under
  its own lock (`not-armed` / `session-mismatch` / `seq-beyond-tail` refused,
  repeats idempotent), and `N2_RING_ACK_SHELL|…|advanced=…` is the only line
  that reports an advancement. The dump channel is
  consumed by the RUNNING extension's watcher tick, so it needs a live VPN
  session — it does not work after teardown. `hdc shell cat` cannot read the
  sandbox and the App cannot write `/data/local/tmp`; the basis is recorded in
  the design doc §5.4.
* **Host verification (no device)** — `cargo test --offline --locked` in
  `client/core`: the `n2_cd`/`n2_ring` unit tests, the integration test
  `tests/n2_cd.rs` (which drives the REAL `N2DestinationTee::consume_control`
  and the real socket-trace receipts end to end:
  `the_real_tee_hits_drive_the_sandwich_and_the_core_owns_the_receipt`,
  `the_switch_baseline_is_read_from_the_cores_own_live_registration`,
  `a_d_plan_is_pre_set_on_wifi_and_advances_through_prepare_ready_and_the_identified_switch`,
  `a_wb_base_identity_exists_only_after_the_real_protect_dup_adopt_chain`,
  `a_wb_plan_takes_its_substitute_receipt_from_the_real_takeover`,
  `a_wb_protect_before_the_pre_hit_cannot_be_rescued_by_a_late_takeover`,
  `a_wb_exempted_receipt_needs_a_successful_protect_and_a_validated_takeover`,
  `a_shell_hit_without_the_tee_is_unsat_and_a_missing_receipt_times_out`), plus
  `tests/n2_cd_state_revision.rs` (8 cases) and `tests/n2_ring_integrity.rs`
  (12 cases) for the rewritten state machine and the ring's durable-seq / ACK /
  rotation integrity (including the explicit ACK's session binding, its
  `not-armed` / `seq-beyond-tail` refusals, idempotency and the fact that every
  read path leaves the watermark alone); the log-format baseline
  (`N2_CD_SANDWICH` phases + `N2_RING_*`); and the shell selfchecks
  `run-n2-cd.mts`, `run-n2-ring.mts` and the N16–N25 group of
  `run-exclusion-generation.mts` (the REAL EntryAbility six-channel rule, the
  REAL armed plan poll + identity + switch hook, the switch-instant step poll,
  the ring forwarding, the seq-paged fetch with manifest — which advances
  nothing — and the explicit ACK channel with all of its named refusals).
* **Not proven** — no device measurement was made. The on-device readability of
  `bearerTypes`/link addresses (B5 is fixed in code, unverified on hardware), the
  availability of `hdc file recv -b` on the target ROM, and whether the rewritten
  orchestration satisfies the frozen「紧前」wording are open for a re-review
  (design doc §4); the affected cells stay `UNSAT` until then.
