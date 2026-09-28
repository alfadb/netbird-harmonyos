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
