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
assert(rust('wg_device.rs').includes('n2_socket_trace::duplicated(wg_socket_raw, fd)') &&
  rust('wg_device.rs').includes('n2_socket_trace::duplicated(raw_fd, fd)') &&
  rust('wg_device.rs').includes('n2_socket_trace::selected_wg(fd)') &&
  rust('wg_device.rs').includes('n2_socket_trace::first_io(fd, std::net::SocketAddr::from((dst.0, dst.1))') &&
  rust('wg_device.rs').includes('"wg_sendto"'), 'WG base and selected sends retain identity and outcome');
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
assert(shell.includes(
  "'status=%{public}s|elapsed_ms=%{public}d|protect_generation=%{public}d|net_id=%{public}d|' +") &&
  shell.includes("'local=%{public}s|wall_ms=%{public}d|diag=true|is_evidence=false',"),
  'protect_settle carries status/protect_generation/local (item 6)');
assert(!shell.includes("'result=%{public}s|elapsed_ms=%{public}d|generation=%{public}d|"),
  'protect_settle no longer uses result=/generation= (item 6)');
assert(shell.includes(
  "'VPN_WG_DATAPLANE_FED|requestId=%{public}s|wgFd=%{public}d|tunFd=%{public}d|deviceUp=true|' +") &&
  shell.includes("'mono_ms=%{public}d|wall_ms=%{public}d',"),
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
