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
NAPI `n2_sandwich_pre(kind, marker, post_marker)`, `napi.rs:754-758`) and hands
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
N2_TUN_TEE_STATS|frames=…|decode_fail=…|dropped=…|mono_ms=…|diag=true|is_evidence=false
N2_TUN_GAP|from_mono=…|to_mono=…|reason=…|diag=true|is_evidence=false
N2_TUN_DEST|scope=session|mono_ms=…|dst=…|proto=…|port=…|frames=…|delta=…|sources=…|diag=true|is_evidence=false
N2_TUN_DEST|scope=session|mono_ms=…|dst=…|proto=any|port=any|frames=0|delta=0|sources=|diag=true|is_evidence=false
N2_TUN_FRAMES|scope=session|mono_ms=…|seen=…|control=…|production=…|leaks=…|diag=true|is_evidence=false
```

* `N2_TUN_TEE_STATS` — `wg_device.rs::N2DestinationTee::stats_line:520-524`,
  emitted on the `N2_TUN_DEST` session cadence (`wg_device.rs::tick:1381-1404`,
  5 s tick
  gate then "counters changed or 30 s elapsed"). `frames` counts every **from-OS**
  frame handed to this tee since the arm (to-OS frames belong to the collector
  tee and are not counted here); `decode_fail` counts the frames
  `n2h::parse_ipv4_frame` refused
  (`wg_device.rs`'s `impl TunFrameTee for N2DestinationTee::on_tun_frame:616-653`);
  `dropped` counts the frames that decoded but were not attributed to a frozen
  capture destination (the tee's own filter, not silent loss). Exact invariant:
  `frames = decode_fail + dropped + Σ N2_TUN_DEST.frames`. **A non-zero
  `decode_fail` makes the interval incomplete → `invalid/UNSAT`.**
* `N2_TUN_GAP` — `wg_device.rs::gap_line:603-605` (thin `N2DestinationTee::gap:500-502`).
  `from_mono`/`to_mono` are the
  monotonic bounds of an interval in which the tee observed NOTHING. Reasons:
  `before-tun-attach` (the tee mount; the pre-attach interval, bounds
  `n2_gap_start` → attach, `wg_device.rs:2408`), `tun-replaced` (a TUN fd
  replacement, `:2330`), `connector-stop` (the tee detach; a zero-width marker
  at the close instant, `:2530` — the unobserved interval then extends to the
  next mount), plus the fault tokens `tun-fd-missing` / `tun-poll-error` /
  `tun-read-error` (`:1279,1286,1303`). The frozen anchor
  `N2_TUN_GAP|reason=before-tun-attach` is unchanged (only the two field names
  became `from_mono`/`to_mono`).
* `N2_TUN_DEST` / `N2_TUN_FRAMES` — `wg_device.rs::dest_line:575-582` /
  `dest_zero_line:584-589` / `frames_line:592-594`. `proto`/`port` are the IP
  protocol number
  and destination port (`proto=0|port=0` is the ≥64-group overflow row);
  `sources` is up to 8 distinct source IPv4 addresses, comma separated;
  `delta` is `frames - frames_at_last_output`. The zero-hit heartbeat row is
  not a sent/received-packet receipt.

## 4/5. `N2_A_EVENT` — the A-column event (T7a items 4 and 5)

`client/core/src/n2_a_event.rs::format_line:245-251` (emitted by
`record_begin:273-277` and `record:263-267`).

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
| `reason` | `management-rpc`, `get-server-key`…, `healthcheck-watch`, `signal-heartbeat-watch`, `dns-requery`, `ok`, `server-healthcheck-answered`, `watchdog-heartbeat-sent`, `healthcheck-wait-exceeded`, `heartbeat-wait-exceeded`, `watch-already-armed`, `watch-arm-refused`, `no-live-management-connection`, `no-live-signal-connection`, `no-live-relay-connection`, `no-connector`, `prior-stimulus`, `not-armed`, `invalid-kind`, `invalid-id` |
| `detail` | `get-server-key`, `get-server-key-failed`, `receipt-in-N2_DNS`, `requery-refused`, `production-echo`, `natural-heartbeat`, `wait_ms=90000` (R), `wait_ms=45000` (S), `gate`, `one-watch-per-session`, `no-n2-sock-registration`, `no-trigger-seam`, `prior-n2-stimulus`, `prior-a-event` |
| `mono_ms` | monotonic ms at emission |

**Event window (item 4).** `phase=begin` opens the event window on that
`dup_fd` (`n2_socket_trace::event_window_begin`, `n2_socket_trace.rs:74-79`) and
the matching `phase=end` closes it. EVERY write completed on that descriptor
inside the window is receipted unconditionally, with `event_id=<id>` (see §6) —
so the one production write a trigger causes can never be swallowed by the
post-connect receipt throttle. For R the window stays open until
`healthcheck_answered()` (`relay_client.rs` EchoHealthCheck branch, called AFTER
the echo write) and for S until the natural heartbeat is observed, so the
response write is inside its window by construction.

**`S` (item 5, frozen v4.1 §4 S row (i)).** `kind=S` sends NOTHING: it arms a
watcher (`Trigger::arm_s_watch`) whose box is `S_WAIT_MS = 45000` ms
(`n2_a_event.rs:97`; arm path `apply:297-427`). The registered receive watchdog
calls
`n2_a_event::natural_heartbeat_about_to_send()` (`n2_a_event.rs:468-482`) /
`natural_heartbeat_sent()` (`:484-500`) right around its own silence-triggered
`send_heartbeat()` (`signal.rs:1074-1100`, `Step::Silence if !probed`), so only a
production frame the client would have sent anyway can complete the event:
`phase=end|result=observed|reason=watchdog-heartbeat-sent|detail=natural-heartbeat`,
or `phase=end|result=timeout|reason=heartbeat-wait-exceeded|detail=wait_ms=45000`.
`signal::request_receive_probe()` is kept as a product-internal seam and is NOT
called by the N2 path.

Shell receipt (not producible from Rust):
`N2_A_EVENT_SHELL|requestId=…|id=…|kind=…|core_ok=…|core_result=…|reason=…|fd=…|dup_fd=…|diag=true|is_evidence=false`
(`NetBirdN2AEvent.ets::formatN2AEventShellLine:125-134`).

## 6. Socket / data-plane fields (T7a item 6)

```
N2_SOCK|phase=subsequent_io|kind=M|S|R|request_id=…|fd=…|dup_fd=…|protect=success|failed|protect_ms=…|protect_generation=…|local=…|remote=…|proto=tcp|net_id=…|bytes=…|mono_ms=…|wall_ms=…|status=sent|detail=post_connect_write|event_id=…|diag=true|is_evidence=false
N2_FD_SEND|kind=Wb|Ws|fd=…|dup_fd=…|packets=…|bytes=…|mono_ms=…|diag=true|is_evidence=false
N7_WG_FEED|adopted|peers=…|endpoints=…|egress_replayed=…|mono_ms=…
```

* `N2_SOCK|phase=subsequent_io` —
  `n2_socket_trace.rs::subsequent_io_line:237-246` (via `post_connect_write:339-365`).
  `event_id` is the A-event id while an event window is open and `none`
  otherwise; the `fd` is always the shell ORIGINAL and `dup_fd` the core-owned
  descriptor. `mono_ms`/`wall_ms` are the completion instants. The field set is
  unchanged from the pre-T7a receipt except for the added `event_id`.
* `N2_FD_SEND` — `n2_socket_trace.rs::fd_send_fields_line:405-410` (via the
  private `fd_send_line:398-403`). Fields stay exactly `packets`/`bytes`/`mono_ms`
  (the judge
  aligns on this shape); `packets=0|bytes=0` on the base descriptor is the
  no-send negative evidence.
* `N7_WG_FEED|adopted` — `wg_device.rs::wg_feed_adopted_line:610-614` (emitted at
  `wg_device.rs:2461-2466`). `peers` /
  `endpoints` are the counts the device carries, `egress_replayed` the number of
  buffered egress sockets actually re-attached at adoption (a failed replay is
  counted out), `mono_ms` the adoption instant.
* Shell-side line (not producible from Rust):
  `VPN_WG_DATAPLANE_FED|requestId=…|wgFd=…|tunFd=…|deviceUp=true|mono_ms=…|wall_ms=…`
  (`NetBirdVpnExtensionAbility.ets:3839-3842`, `feedWgDataPlane`) — `mono_ms` comes from
  the core's `connector_tun_fd_feed` reply (`connector.rs::wg_feed_json:4932-4970`,
  `mono=True`), the same clock the core's `N7_WG_FEED|adopted|…|mono_ms=` uses
  on that very call, so the two lines are alignable; `wall_ms` is
  `Date.now()`.
* Shell-side `N2_SOCK` protect lines (not producible from Rust),
  `NetBirdVpnExtensionAbility.ets::protectSocketWithBox:4106` (`phase=open` at
  `:4118-4121`, `phase=protect_begin` at `:4122-4125`, `phase=protect_settle`
  at `:4176-4195`):
  * `N2_SOCK|phase=open|kind=…|request_id=…|fd=…|protect_generation=…|net_id=…|wall_ms=…|diag=true|is_evidence=false`
  * `N2_SOCK|phase=protect_begin|…` (same fields as `open`)
  * `N2_SOCK|phase=protect_settle|kind=…|request_id=…|fd=…|status=success|failed|elapsed_ms=…|protect_generation=…|net_id=…|local=…|wall_ms=…|diag=true|is_evidence=false`

  The `status=success|failed` vocabulary and the `protect_generation=` field
  name are the core `N2_SOCK` ones, and match the frozen E1 criterion
  (`phase=protect_settle|status=success`); the earlier `result=`/`generation=`
  spellings were unified to them. `local` is the socket's own `getsockname`
  name returned by `n2_socket_trace_protect` (`napi.rs:684-707`), `unknown` when the
  number cannot be inspected — never an invented address.

## 7. Arm-ticket channel (`N2H_ARM_TICKET_*`)

`client/entry/src/main/ets/entryability/EntryAbility.ets::persistCredentialOverrides`
(the arm decision block is at `:172-192`; `N2H_ARM_TICKET_KEPT` at `:183-189`).

* `N2H_ARM_TICKET_WRITTEN|file=netbird-n2h-arm|bytes=…|note=diagnostic-only-non-secret`
  (`EntryAbility.ets:322-326`)
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

`N2_*_SHELL`, the shell `N2_SOCK` phases (`open`/`protect_begin`/
`protect_settle`), `VPN_WG_DATAPLANE_FED`, the `N2H_ARM_TICKET_*` lines and
`N2_STIMULUS*` are produced by ArkTS, not by a Rust formatter, so they cannot
appear in `n2-log-samples.txt`; their exact shapes are listed above and in
`n2-evidence-switch-20260928.md`.
