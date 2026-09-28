import core from 'libnetbird_core.so';
import { iceSocketOpen, mgmtSocketOpen, socketSupplyCloseFd } from './NetBirdConnector';
import { N2EvidenceOptions, validateN2Control } from './NetBirdN2Evidence';

export type N2ControlProtocol = 'tcp' | 'udp';

interface N2ControlReply {
  ok: boolean;
  status?: string;
  error?: string;
  leaks?: number;
}

// Deliberately uses the production opener, but never calls protect or binds a
// network handle. The core registers the marker before connect/send and drops
// the matching from-OS frame before WireGuard. No remote receiver is needed.
export function sendN2Control(armed: N2EvidenceOptions | undefined, target: string,
  protocol: N2ControlProtocol, id: string): void {
  if (armed === undefined) { throw new Error('n2-not-armed'); }
  const error = validateN2Control(armed.capture32, target, id);
  if (error.length > 0) { throw new Error(error); }
  const opened = protocol === 'tcp' ? mgmtSocketOpen() : iceSocketOpen();
  if (opened.fd < 0) { throw new Error('n2-control-open-failed'); }
  try {
    const reply: N2ControlReply = JSON.parse(core.n2_control_send(opened.fd, target, protocol, id)) as N2ControlReply;
    if (!reply.ok) { throw new Error(reply.error ?? 'n2-control-send-failed'); }
  } finally {
    socketSupplyCloseFd(opened.fd);
  }
}

async function waitN2ControlHit(id: string): Promise<void> {
  for (let attempt = 0; attempt < 30; attempt++) {
    const reply: N2ControlReply = JSON.parse(core.n2_control_status(id)) as N2ControlReply;
    if ((reply.leaks ?? 0) > 0) { throw new Error('n2-production-leak'); }
    if (!reply.ok || reply.status === 'miss' || reply.status === 'unknown') {
      throw new Error(reply.error ?? 'n2-control-miss');
    }
    if (reply.status === 'hit') { return; }
    await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 100); });
  }
  throw new Error('n2-control-timeout');
}

// Explicit invocation only around a real production send. A clean sandwich
// requires pre HIT before invoking sendProduction and post HIT afterwards;
// this does not itself claim a network-side receipt or a campaign pass.
export async function withN2Sandwich<T>(armed: N2EvidenceOptions | undefined,
  target: string, protocol: N2ControlProtocol, eventId: string,
  sendProduction: () => Promise<T>): Promise<T> {
  const pre = `${eventId}-pre`;
  const post = `${eventId}-post`;
  sendN2Control(armed, target, protocol, pre);
  await waitN2ControlHit(pre);
  try {
    return await sendProduction();
  } finally {
    sendN2Control(armed, target, protocol, post);
    await waitN2ControlHit(post);
  }
}
