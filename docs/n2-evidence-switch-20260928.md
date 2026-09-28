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

An absent `n2_evidence` key is inert. An armed ticket sets `trustedApplications` to this App alone and removes the template LAN exclusion and endpoint exclusions; each specified IPv4 receives a non-excluded `/32`, in the initial and recreate configs. The capture list is independent of legacy S6 `diag_capture_route`, which must be off. A true S2 `diag_blocked_self`, true S6, `forceDefaultRoute=true`, explicit `endpoint_exclusions_enabled=true`, malformed/duplicate addresses, and a captured `hdc_control_ip` without `allow_hdc_overlap=true` refuse startup. If the hdc address is unknown, the code cannot infer it: record it explicitly before the authorized smoke; do not treat omission as proof of non-overlap. Even with an explicit overlap override, the required nonlisted-app/hdcd device check must pass before relying on the scheme. The default route is held even if the peer becomes ready, and the initial/recreate configuration rejects a remaining default route. Any route masked by an exclusion fails before create. A recreate is marked `N2_EVIDENCE_RECREATE_DETECTED`; this smoke is primarily about the first generation, not E-cell acceptance.

The existing `protect_probe` works independently of S6. Run separate authorized windows with identical frozen destination and `mode:"none"` then `mode:"protect"`; do not conflate a sent request with an observed packet. When `n2_evidence` is armed, the `N2_TUN_DEST|scope=session` counter is attached before the TUN feed and survives collector ARM/CLOSE for the whole connector/VPN session (including a TUN replacement). It counts from-OS frames by destination IPv4, IP protocol and destination port; every 5 seconds it logs cumulative `frames`, since-last-output `delta`, monotonic `mono_ms` and up to 8 distinct source IPv4 addresses per group, if counts changed, and otherwise at least once every 30 seconds. Each destination has at most 64 named protocol/port groups plus one `proto=0|port=0` overflow group. A `proto=any|port=any|frames=0|delta=0` line is a zero-hit heartbeat, not a sent/received-packet receipt. Compare cumulative counts and their deltas across the actual send intervals in (a), (b) and (c), including intervals outside the collector window; unchanged totals alone do not prove a socket sent. Collector CLOSE still emits the legacy `N2_TUN_DEST` window-only summary without `scope=session`, and `frames=0` there only describes that short window. Both logs are diagnostic (`diag=true|is_evidence=false`) and cannot identify UID from TUN packets or change frozen evidence bytes. `N2_PROC_NET` probes `/proc/net/{tcp,udp,tcp6,udp6}` and `/proc/self/net/{tcp,udp,tcp6,udp6}` inside the extension, logging readability, line count and UID from `/proc/self/status` if available, not contents; permission denial is a diagnostic result, not a gate pass. The collector document's `unsat_note` is diagnostic context only; protect status and acceptance criteria come from the current preregistration, not an old platform-permission assumption.

## T3 positive controls and sandwich

`n2_evidence.outer_destinations` is required when armed: a nonempty list (maximum 128) of unique `tcp/IP:port` or `udp/IP:port` targets, each IP in `capture_32`. The core refuses missing/invalid targets before TUN attach, freezes the exact protocol/IP/port set for this session, and emits `N2_TUN_LEAK|verdict=fail` immediately on a matching *non-control* from-OS frame regardless of source IP. The shell and core keep default-unarmed behavior unchanged. An incomplete outer set is **not** proof of absence; compare it with every observed production endpoint throughout the campaign and stop as `UNSAT` on drift.

`NetBirdN2Control.ets` exports the explicit `withN2Sandwich(capture32, target, proto, eventId, production)` convention. Only an already-armed session may call it, and `production` must be one real production send callback, not a synthetic substitute. It sends and waits for a **pre** HIT, invokes and awaits that callback, then sends and waits for a **post** HIT; exceptions or MISS abort that event. No production path invokes this automatically. Each `sendN2Control` opens an unprotected fd through the same production shell opener (`mgmtSocketOpen` for TCP, `iceSocketOpen` for UDP), registers before I/O, and closes the shell fd in `finally`. TCP matches a registered local-port SYN during a bounded window; UDP matches the exact `N2CTRL:<marker>` payload and destination. Neither needs a listener or a special test port. Native `service_tun` consumes a matched frame before either the collector tee or WG encryption, logs `N2_CONTROL_HIT`/`N2_CONTROL_MISS`, and maintains independent `N2_TUN_FRAMES` control/production counts. `n2_control_status` reports cumulative `leaks`; the sandwich aborts when it sees a nonzero value. A negative/protected socket observation cannot be accepted without both ordered HITs for that event and independent send receipts.

`N2_TUN_GAP` records the pre-attach period and TUN replacements (monotonic bounds). A recorded gap overlapping a production event, a failed read/decode, missing control, unknown source attribution, unmatched target or independently observed packet loss makes the relevant interval `invalid/UNSAT`; a zero counter is not a pass. The core does not prove kernel queue integrity or physical receipt, and the bounded per-event control window cannot substitute for independent full-campaign capture/drop accounting. T3 tests use fake TUN frames only and perform no device/network campaign.

## Outer-socket lifecycle receipts (T2/T4)

Only an accepted `n2_evidence` ticket arms `N2_SOCK|` logging; an absent or refused ticket leaves the old log stream unchanged. `phase=open` and `phase=protect_begin` are emitted by the ArkTS protect wrapper for an opened fd, `phase=protect_settle` records the bounded result and `elapsed_ms`. `phase=first_io_begin` is recorded immediately before TCP `connect` or UDP `send`; the core's `phase=first_io|status=connected|sent|failed` receipt is recorded after that call returns. A reused ICE/WG fd records later sends as `phase=subsequent_io_begin` and `phase=subsequent_io` with the same original/dup fd chain and a new destination and completion time. `first_mono_ms` and `first_wall_ms` on completion receipts are **completion timestamps**, not wire timestamps. `fd` is the shell original, `dup_fd` is the core-owned descriptor. `request_id` is the monotonic supply ID for on-demand sockets; initial and spare sockets use the extension request ID. `protect_generation` and `net_id` reflect the shell snapshot at protect completion; `net_id=0` means unavailable, not a verified network. The receipt also gives `local` from `getsockname` after the call, `remote` as numeric address and port, `proto`, `kind` (`M` management, `S` signal, `R` relay, `D` DNS, `I` ICE, `Wb` WG bound socket, `Ws` WG supplied socket), and `detail`. `local=unknown` is not evidence of a physical source. The logs are `diag=true|is_evidence=false` and do not establish a successful protect independently of the platform's actual behavior. A local IPv4 in `100.108.0.0/16` additionally emits `N2_SOCK_OVERLAY_LOCAL`; it is a stop/fail signal, not a substitute for network-side observations.

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
watcher tick (≤ `STATUS_POLL_MS` = 5 s) and consumes it. The arm ticket must be
passed along: a launch without it clears the arm ticket (unchanged default
rule), so a later extension restart would not be armed. Three repetitions = the
same command three times with a fresh `id` each time. The trigger needs the
extension instance that is already armed; a force-stop or a new VPN session
requires a fresh arm first.

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
  other's leftover. If two SEPARATE launches leave both files pending, the
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
The arm ticket must be passed along: a launch without it clears the arm ticket
(unchanged default rule), so a later extension restart would not be armed. The
sandwich itself stays three separate launches with a fresh `id` per leg (the
production event happens between the pre- and the post-control), and
`withN2Sandwich` remains the explicit in-process convention.

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
