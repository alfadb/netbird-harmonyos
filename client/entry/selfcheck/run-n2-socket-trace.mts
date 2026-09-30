// Host-only integration check for the opt-in socket trace handoff.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const shell = readFileSync(join(here, '..', 'src', 'main', 'ets', 'vpnextensionability',
  'NetBirdVpnExtensionAbility.ets'), 'utf8');
const core = join(here, '..', '..', 'core', 'src');
const rust = (file: string) => readFileSync(join(core, file), 'utf8');
const assert = (ok: boolean, label: string) => {
  if (!ok) throw new Error(label);
  console.log('ok ' + label);
};

assert(shell.includes('core.n2_socket_trace_arm(false);') &&
  shell.includes('if (this.n2Evidence !== undefined) {\n      core.n2_socket_trace_arm(true);'),
  'only accepted evidence ticket arms trace');
assert(shell.includes("isDns ? 'D' : request.purpose === 'relay' ? 'R' : request.purpose === 'signal' ? 'S' : 'M', request.id"),
  'on-demand supply correlates kind and monotonic request id');
for (const kind of ['M', 'S', 'I', 'Wb']) {
  assert(shell.includes(`protectSocketWithBox(connection, ${kind === 'M' ? 'mgmtFd' : 'fd'}, '${kind}')`) ||
    shell.includes(`protectSocketWithBox(vpn, opened.fd, '${kind}')`),
  `${kind} original fd is annotated`);
}
assert(rust('napi.rs').includes('reg!("n2_socket_trace_protect", napi_n2_socket_trace_protect);'),
  'NAPI bridge registered');
assert(rust('mgmtsock.rs').includes('n2_socket_trace::duplicated(raw, fd_dup)') &&
  rust('mgmtsock.rs').includes('n2_socket_trace::io_begin(dup, connect_addr)'),
  'TCP dup and connect preserve original-to-dup chain');
assert(rust('protected_dns.rs').includes('n2_socket_trace::first_io(socket.as_raw_fd(), addr.into()') &&
  rust('ice.rs').includes('n2_socket_trace::first_io(dup, remote.into(), "sent"'),
  'DNS and ICE record completed UDP sends');
assert(rust('socket_supply.rs').includes('n2_socket_trace::closed(fd);'),
  'shell original fd is removed on close');
// N2 C/D (Wb): the base socket's liveness is the CORE TAKEOVER (`adopted`,
// which mirrors the registration AND registers the live identity / feeds the
// protect-settle substitute receipt), because the base socket never sends.
assert(rust('wg_device.rs').includes('n2_socket_trace::adopted(wg_socket_raw, fd)') &&
  rust('wg_device.rs').includes('n2_socket_trace::duplicated(raw_fd, fd)') &&
  rust('wg_device.rs').includes('n2_socket_trace::selected_wg(fd)') &&
  rust('wg_device.rs').includes('n2_socket_trace::first_io(fd, std::net::SocketAddr::from((dst.0, dst.1))') &&
  rust('wg_device.rs').includes('"wg_sendto"'),
  'WG base takeover + selected sends retain identity and outcome');
assert(shell.includes("from './NetBirdN2Control'") &&
  shell.includes('sendN2Control(this.n2Evidence, target, protocol, id);'),
  'extension exposes armed-only positive control sender');

// ── N2 T7a format baseline (items 1-8) ──────────────────────────────────────
// The exact shapes docs/n2-log-formats-20260928.md promises, pinned on the
// shipped sources so a silent format drift fails here as well as in
// client/core/tests/n2_log_samples.rs.
assert(rust('wg_device.rs').includes(
  'N2_TUN_TEE_STATS|frames={}|decode_fail={}|dropped={}|mono_ms={now_ms}|diag=true|is_evidence=false'),
  'TUN tee continuity counters exist (item 3)');
assert(rust('wg_device.rs').includes(
  'N2_TUN_GAP|from_mono={from}|to_mono={to}|reason={reason}|diag=true|is_evidence=false'),
  'TUN gap carries from_mono/to_mono (item 3)');
assert(rust('wg_device.rs').includes('tee.gap(from, sys::mono_ms(), "before-tun-attach");') &&
  rust('wg_device.rs').includes('tee.gap(replace_begin, sys::mono_ms(), "tun-replaced");') &&
  rust('wg_device.rs').includes('tee.gap(sys::mono_ms(), sys::mono_ms(), "connector-stop");'),
  'tee mount / TUN replacement / detach each emit a gap (item 3)');
assert(rust('wg_device.rs').includes(
  'N7_WG_FEED|adopted|peers={peers}|endpoints={endpoints}|egress_replayed={egress_replayed}|mono_ms={mono_ms}'),
  'the adopted line carries endpoints/egress_replayed/mono_ms (item 6)');
assert(rust('n2_socket_trace.rs').includes(
  '|bytes={bytes}|mono_ms={mono_ms}|wall_ms={wall_ms}|status=sent|detail=post_connect_write|event_id={event_id}|diag=true|is_evidence=false'),
  'the post-connect receipt carries event_id (item 4)');
assert(rust('n2_socket_trace.rs').includes('let event_id = event_window(fd);') &&
  rust('n2_socket_trace.rs').includes('let due = event_id.is_some()'),
  'a write inside an event window bypasses the throttle (item 4)');
// N2 first batch C/D columns (2026-09-29): the field set is unchanged, but the
// line is now composed as a template literal so the SAME string can be handed
// to the armed-only evidence ring (`n2RingAppend`) — one literal, no drift.
assert(shell.includes("|status=${outcome ? 'success' : 'failed'}|` +") &&
  shell.includes("|local=${local}|wall_ms=${Date.now()}|` +") &&
  shell.includes("`diag=true|is_evidence=false`;"),
  'protect_settle carries status/protect_generation/local (item 6)');
assert(!shell.includes("'result=%{public}s|elapsed_ms=%{public}d|generation=%{public}d|") &&
  !shell.includes("|generation=${"),
  'protect_settle no longer uses result=/a bare generation= (item 6)');
assert(shell.includes("this.n2RingAppend(settleLine);"),
  'the protect_settle line is also recorded in the armed-only evidence ring');
// N2 frozen E1 (Wb substitute evidence, 2026-09-30): the CORE takeover emits its
// OWN real protect-settle row on EVERY Wb takeover (plan or not) — the validated
// core dup, proto=17 and the protect-settle instant (never the later takeover
// clock, never a wall field), with a failed protect reported as failed.
assert(rust('n2_socket_trace.rs').includes('if let Some(line) = takeover(from, to) {') &&
  rust('n2_socket_trace.rs').includes(
    'meta.net_id, if meta.ok { "success" } else { "failed" }, "core-adopt",') &&
  rust('n2_socket_trace.rs').includes('if phase == "protect_settle" {') &&
  rust('n2_socket_trace.rs').includes('format!("protect_mono_ms={mono_ms}")'),
  'the core Wb takeover emits its own protect-settle row (frozen E1)');
const sampleFixture = readFileSync(join(here, '..', '..', 'core', 'tests', 'fixtures',
  'n2-log-samples.txt'), 'utf8');
const settleRow = sampleFixture.split('\n')
  .find((line) => line.startsWith('N2_SOCK|phase=protect_settle|'));
assert(settleRow !== undefined &&
  settleRow.includes('|kind=Wb|request_id=wg_12|fd=21|dup_fd=22|protect=success|') &&
  settleRow.includes('|protect_mono_ms=1234|') &&
  settleRow.includes('|proto=17|net_id=21|status=success|detail=core-adopt|') &&
  !settleRow.includes('wall_ms'),
  'the core protect-settle sample carries the E1 fields and no wall clock');
assert(shell.includes(
  "`VPN_WG_DATAPLANE_FED|requestId=${this.requestId}|wgFd=${wgFd}|` +") &&
  shell.includes("`mono_ms=${typeof tun.mono_ms === 'number' ? tun.mono_ms : -1}|wall_ms=${Date.now()}`;"),
  'VPN_WG_DATAPLANE_FED carries an alignable mono_ms and wall_ms (item 6)');
assert(rust('connector.rs').includes('jinum("mono_ms", crate::sys::mono_ms() as i64)'),
  'the core feed reply supplies that mono_ms from its own clock (item 6)');
assert(rust('napi.rs').includes('jstr("local", &local)'),
  'n2_socket_trace_protect answers the socket local name (item 6)');
assert(readFileSync(join(here, '..', '..', 'core', 'tests', 'n2_log_samples.rs'), 'utf8')
    .includes('n2-log-samples.txt') &&
  readFileSync(join(here, '..', '..', '..', 'docs', 'n2-log-formats-20260928.md'), 'utf8')
    .includes('N2_TUN_TEE_STATS|frames=…|decode_fail=…|dropped=…|mono_ms=…'),
  'the format baseline doc and its executable sample test exist (item 8)');
