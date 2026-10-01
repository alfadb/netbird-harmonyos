# N2 log-format baseline (T7a fixes, 2026-09-28)

The **format contract** for every N2 diagnostic line touched by the T7a
conformance fixes: exact field names, order, value sets and units, with the
source line of the formatting function that produces it. The executor/judge
parses device logs against THIS file; the shell/judge must never have to guess
a field name.

Machine-checkable half: `client/core/tests/n2_log_samples.rs` calls the real
formatters once per Rust-side line and asserts the result byte-for-byte against
[`n2-log-samples.txt`](../client/core/tests/fixtures/n2-log-samples.txt) (one sample per line, with a
`# label` comment above each). Regenerate after an intentional format change
with `N2_LOG_SAMPLES_REGEN=1 cargo test --offline --locked --test n2_log_samples`.

All lines are diagnostic: every one ends `|diag=true|is_evidence=false` and
none of them is N2/N6 evidence or a pass on its own.

## 1. `N2_SANDWICH` — the I/Ws sandwich ordering (T7a item 1)

`client/core/src/n2_sandwich.rs::format_line:165-172` (emitted by `record:174-184`),
documented at `n2_sandwich.rs:30-42`.

```
N2_SANDWICH|kind=I|Ws|phase=pre|post|first-send|marker=…|event_ref=…|result=…|reason=…|fd=…|dup_fd=…|remote=…|wait_ms=…|mono_ms=…|diag=true|is_evidence=false
```

| field | values / unit |
| --- | --- |
| `kind` | `I` or `Ws` (the two A-column sandwich rows; anything else is `rejected|invalid-kind`) |
| `phase` | `pre` (leg registration) / `post` (the leg closes) / `first-send` (the frozen before-attach UNSAT, emitted with no leg) |
| `marker` | **the marker of THAT phase**: the PRE control's own id on `phase=pre`/`first-send`, the **POST control's own id** on `phase=post`. The two are different `[A-Za-z0-9_-]{1,64}` ids; a reused id would be refused by the control registry (`duplicate-control`) |
| `event_ref` | the observation the phase is anchored to: `attach@<mono_ms>` (pre), `first-send@<mono_ms>` (post/first-send), `pre@<mono_ms>` (post timeout), `none` (no first send) |
| `result` | `registered` / `observed` / `unsat` / `timeout` / `rejected` |
| `reason` | `ok`, `control-hit-after-first-send`, `post-control-miss`, `pre-control-miss`, `first-send-wait-exceeded`, `first-send-before-attach`, `first-send-before-pre-control`, `pre-control-not-before-first-send`, `no-attach-anchor`, `leg-already-registered`, `invalid-kind`, `invalid-marker`, `invalid-post-marker`, `invalid-result`, `not-armed` |
| `fd` / `dup_fd` | the shell ORIGINAL / the core-owned descriptor of the first real send (`-1` when none) — the same pair `N2_SOCK` reports |
| `remote` | `ip:port` of the first real send, or `unknown` |
| `wait_ms` | `3000` (pre) or `90000` (post) — the documented bound at that phase (`n2_sandwich.rs:63,68`) |
| `mono_ms` | monotonic ms at emission (`sys::mono_ms()`) |

The core registers BOTH markers at `pre` time (`n2_sandwich.rs::pre:251-306`,
NAPI `n2_sandwich_pre(kind, marker, post_marker)`, `napi.rs:788-793`) and hands
the post marker back through `n2_sandwich_poll()`
(`connector.rs::n2_sandwich_poll_json:5495-5504`, `n2_sandwich.rs::poll:327-375`),
so the shell sends the POST control
with an id the registry has never seen. Shell receipt:
`N2_SANDWICH_SHELL|kind=…|phase=…|marker=…|target=…|result=…|reason=…|diag=true|is_evidence=false`
(`NetBirdN2Sandwich.ets::formatN2SandwichShellLine:127-133`).

## 2. `N2_CONTROL_*` — the positive-control registry (T7a item 2)

`client/core/src/n2_control.rs::hit_line:127-131`, `miss_line:133-137`,
`sent_line:140-143`, `leak_line:146-150`.

```
N2_CONTROL_SENT|id=…|marker=…|dst=…|proto=udp|tcp|source_port=…|mono_ms=…|diag=true|is_evidence=false
N2_CONTROL_HIT|id=…|marker=…|dst=…|proto=udp|tcp|mono_ms=…|diag=true|is_evidence=false
N2_CONTROL_MISS|id=…|marker=…|dst=…|proto=udp|tcp|mono_ms=…|diag=true|is_evidence=false
N2_TUN_LEAK|verdict=fail|dst=…|proto=udp|tcp|mono_ms=…|leaks=…|diag=true|is_evidence=false
```

* `id` is the **request id** the operator asked for
  (`N2_CONTROL_SHELL|id=…`, the `id` of the `n2_control` launch payload, and the
  third argument of `n2_control_send`). The registry keys a control by exactly
  that id, so `marker` is ALWAYS the same value on HIT/MISS/SENT — both fields
  are emitted so a judge can look the request up by id without knowing the
  marker convention. A `marker` that ever differed from `id` would mean a
  registry keyed on something else: treat it as `UNSAT`.
* `dst` is `ip:port`; `proto` is `udp` or `tcp`; `source_port` is the
  registered TCP local port (0/getsockname fallback for UDP).
* `mono_ms` is the registry clock (`sys::mono_ms()`); `N2_CONTROL_MISS` is
  emitted exactly `WAIT_MS = 3000` ms after registration (`n2_control.rs:8`).
* `N2_TUN_LEAK` is emitted for a matching NON-control from-OS frame, never for
  a control.
* Shell delivery receipt (not producible from Rust):
  `N2_CONTROL_SHELL|requestId=…|id=…|target=…|proto=…|result=sent|rejected|reason=…|diag=true|is_evidence=false`
  (`NetBirdN2ControlRequest.ets::formatN2ControlShellLine:114-120`); the
  rejection variant carries `id=none|target=none|proto=none|result=rejected`
  (`:124-128`).

## 3. TUN tee continuity (T7a item 3)

```
N2_TUN_TEE_STATS|frames=…|decode_fail=…|dropped=…|mono_ms=…|diag=true|is_evidence=false|first_fail_len=…|first_fail_byte0=0x…|first_fail_prefix=…|fail_v4=…|fail_v6=…|fail_other=…
N2_TUN_GAP|from_mono=…|to_mono=…|reason=…|diag=true|is_evidence=false
N2_TUN_DEST|scope=session|mono_ms=…|dst=…|proto=…|port=…|frames=…|delta=…|sources=…|diag=true|is_evidence=false
N2_TUN_DEST|scope=session|mono_ms=…|dst=…|proto=any|port=any|frames=0|delta=0|sources=|diag=true|is_evidence=false
N2_TUN_FRAMES|scope=session|mono_ms=…|seen=…|control=…|production=…|leaks=…|diag=true|is_evidence=false
```

* `N2_TUN_TEE_STATS` — `wg_device.rs::N2DestinationTee::stats_line:525-530`,
  emitted on the `N2_TUN_DEST` session cadence (`wg_device.rs::tick:1386-1469`,
  5 s tick
  gate then "counters changed or 30 s elapsed"). `frames` counts every **from-OS**
  frame handed to this tee since the arm (to-OS frames belong to the collector
  tee and are not counted here); `decode_fail` counts the frames
  `n2h::parse_ipv4_frame` refused
  (`wg_device.rs`'s `impl TunFrameTee for N2DestinationTee::on_tun_frame:621-658`);
  `dropped` counts the frames that decoded but were not attributed to a frozen
  capture destination (the tee's own filter, not silent loss). Exact invariant:
  `frames = decode_fail + dropped + Σ N2_TUN_DEST.frames`. **A non-zero
  `decode_fail` makes the interval incomplete → `invalid/UNSAT`.**
  diag-0048 (2026-09-30) appends the tail fields after `is_evidence=false`: the
  FIRST refused frame's `first_fail_len` / `first_fail_byte0` /
  `first_fail_prefix` (lowercase hex of at most its first 4 header bytes; all
  three empty until the first failure) and the per-IP-version refusal buckets
  `fail_v4` / `fail_v6` / `fail_other` (byte 0's high nibble). Only leading
  header bytes are recorded, never the payload; the four pre-existing fields
  keep their names and order.
* `N2_TUN_GAP` — `wg_device.rs::gap_line:608-610` (thin `N2DestinationTee::gap:505-507`).
  `from_mono`/`to_mono` are the
  monotonic bounds of an interval in which the tee observed NOTHING. Reasons:
  `before-tun-attach` (the tee mount; the pre-attach interval, bounds
  `n2_gap_start` → attach, `wg_device.rs:2412`), `tun-replaced` (a TUN fd
  replacement, `:2335`), `connector-stop` (the tee detach; a zero-width marker
  at the close instant, `:2535` — the unobserved interval then extends to the
  next mount), plus the fault tokens `tun-fd-missing` / `tun-poll-error` /
  `tun-read-error` (`:1284,1291,1308`). The frozen anchor
  `N2_TUN_GAP|reason=before-tun-attach` is unchanged (only the two field names
  became `from_mono`/`to_mono`).
* `N2_TUN_DEST` / `N2_TUN_FRAMES` — `wg_device.rs::dest_line:580-585` /
  `dest_zero_line:589-593` / `frames_line:597-600`. `proto`/`port` are the IP
  protocol number
  and destination port (`proto=0|port=0` is the ≥64-group overflow row);
  `sources` is up to 8 distinct source IPv4 addresses, comma separated;
  `delta` is `frames - frames_at_last_output`. The zero-hit heartbeat row is
  not a sent/received-packet receipt.

## 4/5. `N2_A_EVENT` — the A-column event (T7a items 4 and 5)

`client/core/src/n2_a_event.rs::format_line:265-271` (emitted by
`record_begin:294-298` and `record:283-287`).

```
N2_A_EVENT|phase=begin|id=…|kind=M|R|D|S|fd=…|dup_fd=…|remote=…|result=…|reason=…|detail=…|event_id=…|mono_ms=…|diag=true|is_evidence=false
N2_A_EVENT|phase=end|id=…|kind=M|R|D|S|fd=…|dup_fd=…|remote=…|result=…|reason=…|detail=…|event_id=…|mono_ms=…|diag=true|is_evidence=false
```

Field sets are identical on both phases; field order is fixed as above.

| field | values / unit |
| --- | --- |
| `phase` | `begin` = the start of the trigger (M/D before the call, R/S as soon as the arm returned, before any I/O can happen); `end` = the request's outcome. A request refused by a gate emits ONLY `phase=end` (no trigger ever began) |
| `id`, `event_id` | the request id (`[A-Za-z0-9_-]{1,64}`). Both are present on every line with the SAME value; `event_id` is the join name the forced write receipt uses, so no consumer needs to know that |
| `kind` | `M` / `R` / `D` / `S` |
| `fd` / `dup_fd` | the live outer socket pair, taken from the armed trace (`n2_socket_trace::live_of`) — exactly the pair `N2_SOCK` reports; `-1` for a gate refusal or a D event (D's authoritative record is the `N2_DNS` receipt) |
| `remote` | `ip:port`, or `unknown` |
| `result` | `begin` (immediate kinds M/D at begin), `armed` (R/S at begin), `sent` / `error` / `requery` / `rejected` (end), `observed` / `timeout` (end of an R/S watch) |
| `reason` | one of the complete set enumerated under「`reason` — the complete value set」below; a token the `Trigger` seam returned is emitted verbatim |
| `detail` | `get-server-key`, `get-server-key-failed`, `receipt-in-N2_DNS`, `requery-refused`, `production-echo`, `natural-heartbeat`, `wait_ms=90000` (R), `wait_ms=45000` (S), `gate`, `one-watch-per-session`, `watch-arm-refused`, `no-n2-sock-registration`, `no-trigger-seam`, `prior-n2-stimulus`, `prior-a-event` |
| `mono_ms` | monotonic ms at emission |

**Event window (item 4).** The window covers exactly the write the event IS.
For the immediate kinds (M/D) it is opened together with the `phase=begin` line
by `begin_event` (`n2_a_event.rs:307-311`) and the matching `phase=end` closes
it. EVERY write completed on that descriptor inside the window is receipted
unconditionally, with `event_id=<id>` (see §6) — so the one production write a
trigger causes can never be swallowed by the post-connect receipt throttle. For
R the window opens with the arm and stays open until `healthcheck_answered()`
(`relay_client.rs` EchoHealthCheck branch, called AFTER the echo write) or the
90 s box: the server may send its `HealthCheck` at any moment, so only an
already-open window can contain the response write. For `S` the arm opens NO
window — see the `S` paragraph below.

**`S` (item 5, frozen v4.1 §4 S row (i)).** `kind=S` sends NOTHING: it arms a
watcher (`Trigger::arm_s_watch`) whose box is `S_WAIT_MS = 45000` ms
(`n2_a_event.rs:110`; arm path `apply:330-471`, S branch `:376-416`). The arm
emits `phase=begin|result=armed|reason=signal-heartbeat-watch|detail=wait_ms=45000`
through `record_begin` and **opens no event window** (N2 T7a round 3, P1-6):
the heartbeat it waits for is self-scheduled, so an arm-time window would stand
open for the whole 45 s box and hand this event's `event_id` to every other
write on the signal socket, which no judge could then bind to the HEARTBEAT.
The registered receive watchdog calls
`n2_a_event::natural_heartbeat_about_to_send()` (`n2_a_event.rs:519-536`) —
which opens the window on the CURRENT live `S` descriptor — immediately before
its own silence-triggered `send_heartbeat()` (`signal.rs:1074-1110`,
`Step::Silence if !probed`), and `natural_heartbeat_sent()`
(`n2_a_event.rs:567-581`) closes it and completes the event right after that
send returned, so only the write of a production frame the client would have
sent anyway is inside the window:
`phase=end|result=observed|reason=watchdog-heartbeat-sent|detail=natural-heartbeat`,
or `phase=end|result=timeout|reason=heartbeat-wait-exceeded|detail=wait_ms=45000`.
A FAILED heartbeat write closes the window without completing the watch
(`natural_heartbeat_not_sent()`, `n2_a_event.rs:546-560`, called on the
transport-error branch of `signal.rs`): nothing was sent, so nothing may carry
the event id and nothing may be reported as observed. The watch itself stays
armed until that write succeeds or the box expires.
`signal::request_receive_probe()` is kept as a product-internal seam and is NOT
called by the N2 path.

### `reason` — the complete value set (verbatim)

Every possible `reason=` value of an `N2_A_EVENT` line, exactly as it appears on
the wire. `format_line` sanitizes the token (`[A-Za-z0-9._:-]` kept, anything
else replaced by `_`), so the listed spellings are the emitted ones. Tokens
returned by the installed `Trigger` seam are reported verbatim; the shipped
seam is `ConnectorATrigger` (`connector.rs:5379-5441`), whose possible tokens
are all listed here.

| value | phase / result | when |
| --- | --- | --- |
| `not-armed` | end / `rejected` (`detail=gate`) | the N2 evidence switch is not armed — refused before any RPC/syscall |
| `invalid-kind` | end / `rejected` (`detail=gate`) | `kind` is not one of `M`/`R`/`D`/`S` |
| `invalid-id` | end / `rejected` (`detail=gate`) | `id` is empty, longer than 64 bytes, or outside `[A-Za-z0-9_-]` |
| `prior-stimulus` | end / `rejected` (`detail=prior-n2-stimulus` or `prior-a-event`) | D's Q1 gate: an earlier `n2_stimulus` application, or an earlier A event |
| `no-connector` | end / `error` (`detail=no-trigger-seam`) | no `Trigger` seam is installed; also the seam's own token when no connector session exists |
| `no-live-management-connection` | end / `error` (`detail=no-n2-sock-registration`) | M: the armed trace has no live `M` descriptor; also the seam's token when there is no logged-in management client |
| `no-live-signal-connection` | end / `error` (`detail=no-n2-sock-registration`) | S: the armed trace has no live `S` descriptor |
| `no-live-relay-connection` | end / `error` (`detail=no-n2-sock-registration`) | R: the armed trace has no live `R` descriptor |
| `watch-already-armed` | end / `rejected` (`detail=one-watch-per-session`) | R or S while that kind's single watch is still open |
| `management-rpc` | begin / `begin` (`detail=get-server-key`) | M's begin line: the read-only `GetServerKey` is about to run |
| `management-rpc-<class>` | end / `error` (`detail=get-server-key-failed`) | M's RPC failed; `<class>` is `network`, `timeout`, `auth`, `request`, `server`, `parse` or `unsupported_url` (`ErrorClass::from_management`, `connector.rs:228-238`) |
| `management-rpc-timeout` | end / `error` (`detail=get-server-key-failed`) | M's RPC exceeded `M_RPC_WAIT_MS`; the same spelling the `timeout` class token has |
| `no-target` | end / `error` (`detail=requery-refused`) | D: the requery primitive had no target |
| `dns-requery` | begin / `begin` (`detail=receipt-in-N2_DNS`) | D's begin line: the protected requery is about to run |
| `ok` | end / `sent` (M, `detail=get-server-key`) or end / `requery` (D, `detail=receipt-in-N2_DNS`) | the trigger completed |
| `signal-heartbeat-watch` | begin / `armed` (`detail=wait_ms=45000`) | S's arm line: observer armed, no window, nothing sent |
| `healthcheck-watch` | begin / `armed` (`detail=wait_ms=90000`) | R's arm line: window open |
| `server-healthcheck-answered` | end / `observed` (`detail=production-echo`) | R: the production echo answered a server-driven `HealthCheck` |
| `watchdog-heartbeat-sent` | end / `observed` (`detail=natural-heartbeat`) | S: the watchdog sent its natural silence-triggered HEARTBEAT |
| `healthcheck-wait-exceeded` | end / `timeout` (`detail=wait_ms=90000`) | R's 90 s box expired |
| `heartbeat-wait-exceeded` | end / `timeout` (`detail=wait_ms=45000`) | S's 45 s box expired |

Two tokens that look like reasons are **`detail`s**, never reasons:
`watch-arm-refused` is the `detail` of an `error` line whose `reason` is the
`arm_s_watch`/`arm_healthcheck_watch` error token verbatim (in the shipped
connector: `no-connector`), and `get-server-key`/`receipt-in-N2_DNS` are the
`detail`s of the M/D trigger lines.

Shell receipt (not producible from Rust):
`N2_A_EVENT_SHELL|requestId=…|id=…|kind=…|core_ok=…|core_result=…|reason=…|fd=…|dup_fd=…|diag=true|is_evidence=false`
(`NetBirdN2AEvent.ets::formatN2AEventShellLine:125-134`).

## 6. Socket / data-plane fields (T7a item 6)

```
N2_SOCK|phase=subsequent_io|kind=M|S|R|request_id=…|fd=…|dup_fd=…|protect=success|failed|protect_ms=…|protect_generation=…|local=…|remote=…|proto=tcp|net_id=…|bytes=…|mono_ms=…|wall_ms=…|status=sent|detail=post_connect_write|event_id=…|diag=true|is_evidence=false
N2_SOCK|phase=protect_settle|kind=Wb|request_id=…|fd=…|dup_fd=…|protect=success|failed|protect_ms=…|protect_generation=…|protect_mono_ms=…|local=…|remote=unknown|proto=17|net_id=…|status=success|failed|detail=core-adopt|diag=true|is_evidence=false
N2_FD_SEND|kind=Wb|Ws|fd=…|dup_fd=…|packets=…|bytes=…|mono_ms=…|diag=true|is_evidence=false
N7_WG_FEED|adopted|peers=…|endpoints=…|egress_replayed=…|mono_ms=…
```

* `N2_SOCK|phase=subsequent_io` —
  `n2_socket_trace.rs::subsequent_io_line:276-281` (via `post_connect_write:442-482`).
  `event_id` is the A-event id while an event window is open and `none`
  otherwise; the `fd` is always the shell ORIGINAL and `dup_fd` the core-owned
  descriptor. `mono_ms`/`wall_ms` are the completion instants. The field set is
  unchanged from the pre-T7a receipt except for the added `event_id`.
* `N2_SOCK|phase=protect_settle` (**core-emitted, `Wb` only**, added
  2026-09-30) — `n2_socket_trace.rs::sock_receipt_line:382` via the takeover
  behind `adopted:235` (`takeover:266`), called from `wg_device.rs`'s
  `adopt`/`reattach_socket` AFTER the duplicated descriptor was validated. One
  row per `Wb` takeover, **whether or not** a C/D plan measures the class — this
  is the row the frozen E1 substitute evidence reads. `fd` is the shell
  ORIGINAL, `dup_fd` the validated core-owned descriptor, `proto=17` (the IP
  protocol number, as frozen E1 spells it — the I/O receipts spell `udp`),
  `protect`/`status` the ACTUAL shell protect verdict (`success` only when the
  protect call succeeded; a FAILED protect reports `failed` and publishes no
  live identity), `protect_generation`/`net_id`/`local` the registration's own
  (`local` is `getsockname` of the core descriptor), `protect_ms` the shell's
  measured protect elapsed and `remote=unknown` (the base socket is
  unconnected).
  **Time fields**: `protect_mono_ms` is the core's `sys::mono_ms()` at the
  protect VERDICT (`protect:195`) — the instant the frozen E1 ordering compares
  against `N2_TUN_GAP|reason=before-tun-attach` and `N7_WG_FEED|adopted|mono_ms`
  (both core `sys::mono_ms`). The row carries NO wall field by design: the only
  wall the core could name at emission is the LATER takeover clock, and it must
  never be presented as the protect wall (the SHELL's own `phase=protect_settle`
  line carries the protect wall as `wall_ms`). The chain with the shell's three
  phases is bound by `fd`/`request_id`/`protect_generation`; the core row adds
  the `dup_fd` that only the validated takeover can name.
* `N2_FD_SEND` — `n2_socket_trace.rs::fd_send_fields_line:552-556` (via the
  private `fd_send_line:545-548`). Fields stay exactly `packets`/`bytes`/`mono_ms`
  (the judge
  aligns on this shape); `packets=0|bytes=0` on the base descriptor is the
  no-send negative evidence.
* `N7_WG_FEED|adopted` — `wg_device.rs::wg_feed_adopted_line:615-618` (emitted at
  `wg_device.rs:2466-2481`). `peers` /
  `endpoints` are the counts the device carries, `egress_replayed` the number of
  buffered egress sockets actually re-attached at adoption (a failed replay is
  counted out), `mono_ms` the adoption instant.
* Shell-side line (not producible from Rust):
  `VPN_WG_DATAPLANE_FED|requestId=…|wgFd=…|tunFd=…|deviceUp=true|mono_ms=…|wall_ms=…`
  (`NetBirdVpnExtensionAbility.ets:4534` `feedWgDataplane`, the line at
  `:4591-4593`) — `mono_ms` comes from
  the core's `connector_tun_fd_feed` reply (`connector.rs::wg_feed_json:4932-4961`,
  `mono=True`), the same clock the core's `N7_WG_FEED|adopted|…|mono_ms=` uses
  on that very call, so the two lines are alignable; `wall_ms` is
  `Date.now()`.
* Shell-side `N2_SOCK` protect lines (not producible from Rust),
  `NetBirdVpnExtensionAbility.ets::protectSocketWithBox:4859-4963` (`phase=open` at
  `:4872-4875`, `phase=protect_begin` at `:4877-4880`, `phase=protect_settle`
  at `:4945-4949`):
  * `N2_SOCK|phase=open|kind=…|request_id=…|fd=…|protect_generation=…|net_id=…|wall_ms=…|diag=true|is_evidence=false`
  * `N2_SOCK|phase=protect_begin|…` (same fields as `open`)
  * `N2_SOCK|phase=protect_settle|kind=…|request_id=…|fd=…|status=success|failed|elapsed_ms=…|protect_generation=…|net_id=…|local=…|wall_ms=…|diag=true|is_evidence=false`

  The `status=success|failed` vocabulary and the `protect_generation=` field
  name are the core `N2_SOCK` ones, and match the frozen E1 criterion
  (`phase=protect_settle|status=success`); the earlier `result=`/`generation=`
  spellings were unified to them. `local` is the socket's own `getsockname`
  name returned by `n2_socket_trace_protect` (`napi.rs:718-741`), `unknown` when the
  number cannot be inspected — never an invented address. These three shell
  rows are the protect-side half of the identity chain (`fd` + `request_id` +
  `protect_generation`); the CORE's own takeover row (§6,
  `phase=protect_settle|kind=Wb`, 2026-09-30) is the half that can name the
  validated `dup_fd` and the shared monotonic clock, and it is emitted whether
  or not a C/D plan measures the class.

## 7. Arm-ticket channel (`N2H_ARM_TICKET_*`)

`client/entry/src/main/ets/entryability/EntryAbility.ets::persistCredentialOverrides`
(the arm decision block is at `:172-192`; `N2H_ARM_TICKET_KEPT` at `:183-189`).

* `N2H_ARM_TICKET_WRITTEN|file=netbird-n2h-arm|bytes=…|note=diagnostic-only-non-secret`
  (`EntryAbility.ets:404-405`)
* `N2H_ARM_TICKET_REMOVED|file=netbird-n2h-arm|note=…` — `note` is
  `default-no-arm`, `failed-write-cleanup`, or another named branch
* `N2H_ARM_TICKET_KEPT|file=netbird-n2h-arm|note=request-launch-without-arm-ticket|diag=true|is_evidence=false`
  — **new (T7a item 7)**: a launch that carries an N2 request parameter
  (`n2_control` / `n2_stimulus` / `n2_a_event`) but NO `n2h_arm_options` KEEPS
  the existing ticket instead of deleting it, so the documented operator flow
  ("arm once, then one launch per request") cannot silently disarm a later
  extension restart. A launch with NO N2 request parameter keeps the original
  `default-no-arm` deletion rule; an explicit ticket is always written verbatim.

## What is NOT in the sample file

`N2_*_SHELL`, the SHELL-side `N2_SOCK` phases (`open`/`protect_begin`/
`protect_settle` — the CORE's own `phase=protect_settle|kind=Wb` row IS in the
sample, §6), `VPN_WG_DATAPLANE_FED`, `VPN_NETWORK_BASELINE` /
`VPN_NETWORK_RECOVERED` / `VPN_NETWORK_RECOVERY_FAIL`, the `N2H_ARM_TICKET_*`
lines, `N2_STIMULUS*`, `N2_CD_SANDWICH_SHELL` and the `n2_ring_dump` receipts
(`N2_RING_DUMP_SHELL`) are produced by ArkTS, not by a Rust formatter, so they
cannot appear in `n2-log-samples.txt`; their exact shapes are
listed above and in `n2-evidence-switch-20260928.md` / the design doc
`n2-cd-app-sandwich-20260929.md`. (`N2_RING_CHUNK` / `N2_RING_DUMP` ARE core
formatters — see §9.)

## 8. `N2_CD_SANDWICH` — the App-side switching sandwich (2026-09-29)

N2 first batch C/D columns. There are TWO formatters in
`client/core/src/n2_cd.rs`:

* `format_line_ext:421-432` is what the **state machine actually emits**
  (via `plan_line:434-440` → `record:442-449` / `close:452-464`);
* `format_line:405-414` is the **compat/baseline** form: the same field
  sequence up to `mono_ms`, without the identity block. It exists so
  `client/core/tests/n2_log_samples.rs` can render the frozen fixture
  (`tests/fixtures/n2-log-samples.txt`) without pinning one run's clocks, and
  its shape must not move.

Design, lifecycle and the T0 questions in
[`n2-cd-app-sandwich-20260929.md`](n2-cd-app-sandwich-20260929.md).

```
N2_CD_SANDWICH|id=…|column=C|D|kind=M|S|R|D|I|Wb|Ws|rep=…|phase=plan|prepare|switch|pre|receipt|post|marker=…|result=…|reason=…|net_before=…|net_after=…|local_before=…|local_after=…|fd=…|dup_fd=…|remote=…|wait_ms=…|mono_ms=…|net_id=…|protect_generation=…|identity=fd:dup_fd:generation|event_mono_ms=…|local=…|request_id=…|target=…|proto=…|detail=…|diag=true|is_evidence=false
```

The baseline form stops after `mono_ms`:

```
N2_CD_SANDWICH|id=…|column=…|kind=…|rep=…|phase=…|marker=…|result=…|reason=…|net_before=…|net_after=…|local_before=…|local_after=…|fd=…|dup_fd=…|remote=…|wait_ms=…|mono_ms=…|diag=true|is_evidence=false
```

| field | values / unit |
| --- | --- |
| `id` | the plan id the operator sent (`[A-Za-z0-9_-]{1,59}`, `MAX_ID`; the two control markers are derived from it as `<id>-pre` / `<id>-post`, which is why the bound is 59 and not 64) |
| `column` | `C` (Wi-Fi → cellular) or `D` (cellular → Wi-Fi) |
| `kind` | the seven frozen outer classes |
| `rep` | the repetition index, `1..999` |
| `phase` | `plan` / `prepare` / `switch` / `pre` / `receipt` / `post` |
| `marker` | **the marker of THAT phase**: `none` on `plan`/`prepare`/`switch`/`receipt`, `<id>-pre` on `pre`, `<id>-post` on `post` |
| `result` | `plan`: `planned` / `rejected`; `prepare`: `prepared` / `ready` / `rejected` / `unsat`; `switch`: `switched` / `rejected` / `unsat`; `pre`: `hit` / `rejected` / `unsat`; `receipt`: `receipt` / `exempted` / `rejected` / `unsat`; `post`: `observed` / `rejected` / `unsat` |
| `reason` | the full set the state machine can answer, per phase — `plan`: `ok`, `await-start-bearer`, `invalid-column`, `invalid-kind`, `invalid-rep`, `invalid-id`, `invalid-target`, `start-bearer-not-wifi`, `plan-already-active`, `plan-already-used`, plus the shell's own `start_verdict` token (`bearer-unknown` / `start-network-mismatch` / `no-target`) and the shell's `not-armed`; `prepare`: `not-a-d-measurement`, `preparation-recovered`, `not-a-d-column`, `prepare-not-owed`, `recovery-not-owed`, `prepare-wait-exceeded`, `prepare-recovery-wait-exceeded`, plus the shell's `bearer-unknown` / `direction-mismatch`; `switch`: `ok`, `switch-already-reported`, `switch-wait-exceeded`, `prepare-hop-required`, `switch-before-preparation-complete`, `dwell-min-not-reached`, `switch-identity-required`, `switch-identity-missing`, `switch-net-id-unmeasured`, `switch-generation-unmeasured`, plus the shell's `direction-mismatch` / `bearer-unknown`; `pre`: `gate-open`, `pre-hit-timeout`, `pre-control-miss`, `pre-hit-unobserved`, `pre-hit-before-switch`, `pre-before-switch`, `pre-already-reported`, `pre-wait-exceeded`, `invalid-result`, plus the shell's own verbatim control-send error token; `receipt`: `ok`, `wb-protect-settle-success`, `receipt-before-pre-hit`, `receipt-before-formal-switch`, `receipt-already-issued`, `invalid-fd`, `generation-unchanged`, `identity-unchanged`, `generation-unmeasured`, `net-id-unmeasured`, `net-id-mismatch`, `protect-failed`, `io-time-unmeasured`, `io-before-pre-hit`, `connect-only`, `wb-protect-settle-missing`, `receipt-wait-exceeded`; `post`: `control-hit-after-receipt`, `post-hit-unobserved`, `post-before-receipt`, `post-control-miss`, `post-wait-exceeded`, `invalid-result`. Every refusal that is NOT a terminal `close()` is a non-terminal `rejected`: the plan keeps waiting. Unarmed entries answer `not-armed` with an EMPTY line (nothing is emitted) |
| `net_before` / `net_after` | the measured physical bearer (`wifi` / `cellular` / `ethernet` / `bluetooth` / `unknown`); `net_after` is `unknown` until a phase measures it |
| `local_before` / `local_after` | the measured network's own link address (`unknown` when the platform could not tell — never an invented address) |
| `fd` / `dup_fd` | the shell ORIGINAL / the core-owned descriptor of the receipt leg on `receipt`/`post` (`-1` before one exists) — the same pair `N2_SOCK` reports; `switch`/`pre` carry `-1` |
| `remote` | `ip:port` of that receipt, or `unknown` |
| `wait_ms` | the phase's written bound: `plan` → `300000` (`SWITCH_WAIT_MS`) or `300000` for D (`PREPARE_WAIT_MS`); `prepare` → `300000` (`PREPARE_WAIT_MS`) / `300000` (`SWITCH_WAIT_MS`) on `ready`; `switch` → `90000` (`RECEIPT_WAIT_MS`) on terminal, `30000` (`PRE_WAIT_MS`) on a non-terminal refusal; `pre` → `30000` (`PRE_WAIT_MS`) terminal, `90000` (`RECEIPT_WAIT_MS`) on `hit`; `receipt` → `90000` (`RECEIPT_WAIT_MS`) refused, `90000` (`POST_WAIT_MS`) issued; `post` → `90000` (`POST_WAIT_MS`) |
| `mono_ms` | monotonic ms at emission; the phases are ordered by these values (plus the tee's own HIT instants) |
| `net_id` | the NEW network's platform net id measured at the switch (`0` = not measured); the receipt must match it |
| `protect_generation` | the shell's `networkEpoch` of the identity the line carries (`0` = not measured) |
| `identity` | `fd:dup_fd:generation` of that identity, or `none` |
| `event_mono_ms` | the monotonic instant of the fact this line reports (switch instant, pre-HIT, receipt I/O, post-HIT…); `0` = not measured |
| `local` | `local` with an empty value spelled `none` (the receipt's own `getsockname` name on receipt/post) |
| `request_id` | the private channel's `requestId`, or `none` |
| `target` / `proto` | the frozen pre-registration (frozen:L42): the control marker's frozen destination and the kind's own protocol (`tcp` for M/S/R, `udp` for the rest), fixed BEFORE any control is sent; `none` when empty |
| `detail` | the receipt's own detail (`stun_txn_…`, `wg_sendto`, `dns`, `protect_settle`, `post_connect_write`), recorded verbatim for the per-kind judge; `none` when empty |

The receipt criteria (all of them enforced in `receipt:787-857`, each refusal
named): the new identity's `generation` must differ from the switch baseline and
so must the `(fd, dup_fd)` pair; `net_id` must equal the switch measurement;
`protect` must have succeeded; the receipt's I/O instant must be LATER than the
pre control's tee HIT; `M`/`S`/`R` must be a real `phase=subsequent_io` business
write (a completed `connect` is `connect-only`); `Wb` needs the NEW generation's
`phase=protect_settle|status=success` (`wb-protect-settle-missing` otherwise)
and closes `result=exempted|reason=wb-protect-settle-success`. The `Wb` receipt
is fed by the CORE TAKEOVER of the validated dup (`n2_socket_trace::adopted`,
called from `wg_device.rs`'s `adopt`/`reattach_socket`): its `dup_fd` is the
taken-over core-owned descriptor and its `event_mono_ms` is that registration's
OWN protect-settle instant, never the takeover clock — so a socket protected
before the pre HIT stays refused `io-before-pre-hit` however late it is adopted.
`Wb` is NOT
exempt from the pre/post controls any more. The six wait bounds are source
constants (`PREPARE_WAIT_MS`/`DWELL_MIN_MS`/`PRE_WAIT_MS`/`RECEIPT_WAIT_MS`/
`POST_WAIT_MS`/`SWITCH_WAIT_MS`); an unarmed session emits nothing at all.

Shell receipt (not producible from Rust):
`N2_CD_SANDWICH_SHELL|requestId=…|id=…|column=…|kind=…|rep=…|phase=…|marker=…|result=…|reason=…|diag=true|is_evidence=false`
(`NetBirdN2CdPlan.ets::formatN2CdPlanShellLine:247-255`); the rejection variant
is `…|id=none|column=none|kind=none|rep=0|phase=plan|marker=none|result=rejected|reason=…`
(`:259-263`). The shell's `phase` mirrors the core's own phase for a terminal
step (`n2_cd_poll`'s `phase` field), so both logs join on the same phase.

## 9. `N2_RING_*` — the armed-only app-internal evidence ring (2026-09-29)

`client/core/src/n2_ring.rs::armed_line:1158-1161`, `stats_line:1164-1168`,
`fail_line:1170-1173`, `recovered_line:1175-1177`, `rotated_line:1179-1181`,
`rotated_span_line:1186-1189`, `lost_line:1192-1196`, `overflow_line:1199-1202`,
`manifest_line:1208-1217`, `chunk_line:1221-1223`, `dump_summary_line:1226-1231`.

```
N2_RING_ARMED|file=…|max_bytes=…|segments=…|bytes=…|mono_ms=…|diag=true|is_evidence=false
N2_RING_STATS|file=…|enabled=…|bytes=…|max_bytes=…|segments=…|writes=…|failures=…|rotations=…|last_error=…|mono_ms=…|diag=true|is_evidence=false
N2_RING_WRITE_FAILED|failures=…|code=errno-<n>|io|mono_ms=…|diag=true|is_evidence=false
N2_RING_RECOVERED|failures=…|mono_ms=…|diag=true|is_evidence=false
N2_RING_ROTATED|segments=…|bytes=…|dropped_from_seq=…|dropped_to_seq=…|dropped_records=…|acked_seq=…|mono_ms=…|diag=true|is_evidence=false
N2_RING_LOST|from_seq=…|to_seq=…|records=…|reason=…|code=…|mono_ms=…|diag=true|is_evidence=false
N2_RING_OVERFLOW|dropped_records=…|from_seq=…|to_seq=…|acked_seq=…|mono_ms=…|diag=true|is_evidence=false
N2_RING_MANIFEST|file=…|sess=…|enabled=…|segments=…|max_bytes=…|bytes=…|records=…|first_seq=…|last_seq=…|frozen_tail_seq=…|next_seq=…|acked_seq=…|writes=…|failures=…|rotations=…|lost_records=…|lost_from=…|lost_to=…|lost_reason=…|dropped_by_ack=…|overflow=…|overflow_events=…|gaps=…|gap_from=…|gap_to=…|malformed=…|tail_open=…|last_error=…|mono_ms=…|diag=true|is_evidence=false
N2_RING_CHUNK|seq=<持久 seq>|of=<count>|line=<原件逐字>
N2_RING_DUMP|from_seq=…|to_seq=…|count=…|emitted=…|frozen_tail_seq=…|first_seq=…|last_seq=…|records=…|bytes=…|malformed=…|lost_records=…|overflow=…|acked_seq=…|result=dumped|rejected|reason=…|diag=true|is_evidence=false
N2_RING_ACK_SHELL|requestId=…|sess=…|up_to_seq=…|acked_seq=…|advanced=true|false|result=acked|rejected|reason=…|diag=true|is_evidence=false
```

* Every persisted record is `N2R|sess=…|seq=…|mono_ms=…|wall_ms=…|line=<原件>`
  (`encode_record:1110-1113`), so a page is addressed by its **durable `seq`**
  (across appends, rotation and restarts) and never by a line index over
  concatenated segments.
* `file` is the **basename** only (the absolute sandbox path is never logged);
  `bytes` is the current segment's size; `max_bytes`/`segments` are the
  configured bounds (defaults 4 MiB / 3, `N2_RING_ARMED` reports them).
* **The ROTATED line the product writes is `rotated_span_line`**, not the bare
  `rotated_line`: it names the seq span that left the ring
  (`dropped_from_seq`/`dropped_to_seq`/`dropped_records`) plus the ACK watermark
  the rotation was gated on. `rotated_line` remains only as the terse form.
* `N2_RING_STATS` is emitted at configuration, **every 60 s while armed**
  (shell-side `N2_RING_STATS_MS = 60000`, core-formatted), at teardown
  (`onDestroy`, after `connectorStop`/`destroyOnce`) and on demand; it IS
  recorded in the ring. `N2_RING_ROTATED` is the FIRST record of the new
  segment; `N2_RING_WRITE_FAILED` (the FIRST failure only) rides hilog with the
  ring tee suppressed — appending it could exceed the cap and rotate again,
  which would announce again. `N2_RING_RECOVERED` (the first success after ≥1
  failure) IS recorded. `N2_RING_LOST` / `N2_RING_OVERFLOW` carry the seq span
  of what was NOT persisted, so a gap can be attributed instead of only counted.
  `N2_RING_MANIFEST` is the head/tail/gap/failure inventory (the shell asks for
  it with `n2_ring_manifest()` before a fetch); it IS recorded.
* `code` is an errno-style token (`errno-<n>`, `io`) — never a path and never
  the OS message text.
* The replay fallback (`n2_ring_dump`, shell-side) re-emits the ring on hilog,
  one `N2_RING_CHUNK|seq=<durable>|of=<count>|line=<verbatim>` per record
  (`line=` is the LAST field; its content may contain `|`) plus ONE
  `N2_RING_DUMP|from_seq=…|to_seq=…|…|emitted=…|…|result=dumped|rejected|reason=…|…`
  summary. Both go to **HiLog only** — neither is written back into the ring.
  The shell's own receipt IS written back:
  `N2_RING_DUMP_SHELL|requestId=…|from_seq=…|to_seq=…|count=…|emitted=…|ack_watermark=…|result=…|reason=…|diag=true|is_evidence=false`
  (`NetBirdN2Ring.ets::formatN2RingDumpShellLine`; the refusal variant
  `…|from_seq=0|to_seq=0|count=0|emitted=0|ack_watermark=-1|result=rejected|reason=…`).
  **A dump NEVER advances the ACK**: `ack_watermark=` is the core's `acked_seq` AS
  IT STANDS at serve time (`-1` when the core refused the page), so re-emitting a
  window proves the App READ it, not that the host persisted and verified it.
  The legacy line-index document (`{"start":…}`) is refused
  `legacy-start-index-unsupported` by both the shell and the core.
* The retrieved watermark moves ONLY through the EXPLICIT host ACK channel
  (`n2_ring_ack`, request document `{"session":…,"up_to_seq":…}` — BOTH keys
  required; shell parser `NetBirdN2Ring.ets::parseN2RingAck`, core parser
  `n2_ring::parse_ack_request`). The shell forwards the document VERBATIM and the
  CORE binds it to the CURRENT arm inside the ring's own lock, so
  `not-configured` / `not-armed` / `invalid-session` / `session-mismatch` (a stale
  arm) / `seq-beyond-tail` are refused with the watermark UNCHANGED, and a repeat
  is idempotent (`advanced=false`). The shell's receipt
  `N2_RING_ACK_SHELL|requestId=…|sess=…|up_to_seq=…|acked_seq=…|advanced=…|result=acked|rejected|reason=…|diag=true|is_evidence=false`
  is the ONLY line that reports an advancement; a document the shell refuses
  never reaches the core and answers
  `…|sess=-|up_to_seq=-1|acked_seq=-1|advanced=false|result=rejected|reason=…`.


## 10. What changed for the pre-existing lines (2026-09-29)

The C/D implementation composes the shell's own `N2_*` lines as template
literals so the SAME string is logged and handed to the ring
(`n2RingAppend`). The emitted text is unchanged; the two lines whose source shape
was pinned by a selfcheck (`N2_SOCK|phase=protect_settle`, `VPN_WG_DATAPLANE_FED`)
keep their exact field set and order.

## 11. The core `Wb` protect-settle row (2026-09-30, frozen E1 substitute evidence)

`n2_socket_trace::adopted` (the takeover behind `WgDevice::adopt` /
`reattach_socket`) now emits the class's OWN `N2_SOCK|phase=protect_settle` row
for every armed `Wb` takeover, via the existing `sock_receipt_line` and
unconditionally of any C/D plan. The row, its field semantics and the
deliberate absence of a wall field are specified in §6. Two consequences for a
judge:

* the frozen E1 identity chain no longer depends on the CD substitute receipt:
  the row is on hilog (and, being a core `hilog::emit`, in the armed-only ring)
  even when no C/D plan is active, so `phase=open` / `phase=protect_begin`
  (shell) → `phase=protect_settle` (shell, original fd) → the core row (same
  `fd`/`request_id`/`protect_generation`, plus the validated `dup_fd`) is
  complete from the log alone;
* the row's `protect_mono_ms` is comparable with the tee anchors
  (`N2_TUN_GAP|reason=before-tun-attach`, `N7_WG_FEED|adopted|mono_ms`), which is
  what the frozen E1 ordering ("`protect_settle` strictly before tee attach")
  needs; the shell's `wall_ms` cannot be compared with those `sys::mono_ms`
  values, and the core row does NOT pretend otherwise.

The two field sets the SHELL emits for `open`/`protect_begin`/`protect_settle`
are untouched, and so are the zero-send counters: the takeover row is a
diagnostic only, `N2_FD_SEND` keeps reporting `packets`/`bytes` for the same
registration.

## 12. `peer=` on the ICE / WG session-lifecycle lines (2026-10-01, device 0050)

Two families of CORE diagnostic lines carry the same 8-character peer label
(`peer_conn::peer_short`: the public key's first eight alphanumeric characters,
so `+`/`/`/`=` from the base64 form are skipped — `FyLw4PSBg1hjr…` →
`FyLw4PSB`). Neither family is in the sample file (both are core diagnostics,
not N2 evidence rows); the exact strings are pinned by their producers' tests
(`peer_conn` for `N5c_ICE`, `wg_device::lifecycle_line_tests` for
`N6_WG_DEVICE`).

**`N5c_ICE` — the local-candidate collection outcome.** Field order is stable;
the `reason=` field is last:

```text
N5c_ICE|locals-gathered|peer=<p>|round=<session-at-ms>|added=<n>|srflx=<n>|advertised=<n>
N5c_ICE|locals-waiting|peer=<p>|round=<session-at-ms>|phase=<initiate|retry|answer>|need=<n>|have=<n>|waited_ms=<n>|reason=insufficient-sockets
N5c_ICE|locals-failed|peer=<p>|round=<session-at-ms>|phase=<initiate|retry|answer>|reason=<error>
```

`locals-waiting` (new in 0050) is the protected-socket pool being SHORT of the
attempt's budget: it is a **wait**, not an error — no `peer-conn: pump error`
line follows it, the peer records no `last_error`, and the waiting time is
excluded from the ICE negotiation start deadline. The first `locals-waiting`
line of a wait is always emitted; the rest are rate-limited to one per 5 s per
peer (`peer_conn::LOCALS_WAIT_LOG_MS`), and `waited_ms` carries how long the
pool has been short, so the throttling loses no timing. The
`reason=insufficient-sockets` token is kept as the last field so a token search
still finds the cause.
`locals-failed` is now reserved for genuine errors (gather/bind/DNS failures).

**`N6_WG_DEVICE` — the WG session lifecycle.** `peer=` is appended at the END
of the line; the earlier fields keep their order (device 0050 could observe
`wgSessions=4` but not WHICH peers those were):

```text
N6_WG_DEVICE|session-established|peer=<p>
N6_WG_DEVICE|session-expired|peer=<p>
```

`N6_WG_DEVICE|adopt|…` / `reattach|…` / `peers=<n>` / `cleared` and the
handshake-campaign lines are unchanged: `adopt`/`reattach` are socket-level
(they belong to no single peer), and `handshake-campaign-deadline` still names
no peer.

## 13. Silent-peer initiate backoff + `ice_sockets.need_per_peer` (2026-10-01, device 0051)

Two additions from 0051 §7, both core-authored and both pinned by their
producers' tests (`peer_conn::tests` for the lines, `connector::tests` for the
status section).

**`N5c_ICE|initiate-backoff` / `N5c_ICE|backoff-reset` — the unresponsive-peer
retry backoff.** 0051 §3.4: two peers whose remote end sent ZERO signal frames
(`TrBfURPG`, `ai4IGfaf`) were re-initiated every ~11 s
(`START_DEADLINE_MS` + `RETRY_COOLDOWN_MS`) forever — 24 identical
`negotiation-start-timeout` rounds in 4 min. The trigger stays the existing
zombie-negotiation verdict (`remote_creds` never arrived within the deadline);
what changes is only WHEN the next initiate may start:

```text
N5c_ICE|initiate-backoff|peer=<p>|misses=<n>|next_ms=<ms>
N5c_ICE|backoff-reset|peer=<p>|reason=<signal|network-change|signal-reconnect>
```

| field | values / unit |
| --- | --- |
| `misses` | consecutive `negotiation-start-timeout` rounds in which the peer sent NO signal frame at all (offer/answer/candidate). A single received frame clears the count (`reason=signal`); one remote frame is progress, never silence |
| `next_ms` | the delay from this timeout to the next initiate: `min(9000 << (misses − 3), 300000)` → `9000/18000/36000/…/300000` (`INITIATE_BACKOFF_BASE_MS = START_DEADLINE_MS`, `INITIATE_BACKOFF_MAX_MS = 300000`). The observed initiate interval is `START_DEADLINE_MS + next_ms`, i.e. ≈18 s at the first level, ≈5 min at the cap |
| `reason` | what invalidated the silence: `signal` (any frame from that peer, incl. a candidate), `network-change` (`restart_for_network_change`), `signal-reconnect` (the signal stream's false→true edge) |

The backoff starts at `INITIATE_BACKOFF_MIN_MISSES = 3` consecutive silent
timeouts (`misses` 1–2 keep the pre-0051 `RETRY_COOLDOWN_MS`). Entering and each
escalation emit one `initiate-backoff` line; because every escalation doubles
the interval, those lines are rate-limited by construction. `backoff-reset` is
emitted at most once per active backoff (the reset clears the flag). Neither
line changes the ANSWER path: a received OFFER is still answered in the same
beat regardless of any backoff, and only the next locally-initiated attempt is
delayed. Peers that never went silent (Connected, or answered by the remote at
least once per round) never enter the backoff.

**`connector_status().ice_sockets.need_per_peer` — the core's own per-peer
socket demand** (a STATUS-document field, not a hilog line). It is
`allowed_ifaces × 2`, the exact input of `peer_conn`'s
`required_local_sockets()` (0051 measured 4 for 2 allowed interfaces and 6 for
3), and `-1` when the interfaces cannot be enumerated. The shell sizes its pool
as `clamp(peers × need_per_peer, 8, 48)` and falls back to its own 4-per-peer
constant on a missing/`-1` field (`NetBirdIceSocketPool.ets`); the `48` cap
covers 8 peers × need 6. The `queued`/`taken` fields and their meaning are
unchanged, and the section is appended last — a reader of the earlier fields
sees the same document.

