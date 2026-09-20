// N2-H iface-bytes availability probe (2026-09-18, diagnostic-only).
//
// What this is: ONE availability experiment for the OFFICIAL per-NIC byte
// counters — @ohos.net.statistics getIfaceRxBytes(nic) / getIfaceTxBytes(nic)
// (SDK 26.0.0.821 @ohos.net.statistics.d.ts:79/113 — since 10, syscap
// Communication.NetManager.Core, NO @permission and NO @systemapi on the two
// iface methods; ohos.permission.GET_NETWORK_STATS is carried ONLY by the
// getUid* methods, which this probe never calls and which belong to a
// different question: what OTHER apps' UIDs can be queried). Device run 0017
// proved the app cannot read /proc/net/dev directly (Permission denied,
// os error 13, collector counters=[]); this probe answers whether the public
// API works where that proc read did not.
//
// What this is NOT: not a C7 replacement claim, not part of the device
// evidence collector, never Evidence. Every line is diag=true|is_evidence=
// false. It samples BYTES only — there is NO public per-iface PACKET API, and
// no packet count is derived, divided out, or back-filled from these bytes.
// Evidence.counters / notes are untouched; no pre/post-ARM reconciliation
// happens. A byte value for 'vpn-tun', or a 0 returned for the nonexistent
// control name, is recorded verbatim and proves nothing on its own — the
// interpretation belongs to the reviewed run document
// (docs/n2h-iface-statistics-probe-20260918.md), not to this file.
//
// Caller contract: the extension's maybeArmN2hCollector SUCCESS branch only
// (the N2H_COLLECTOR_ARMED line) — at most once per diagnostic arm,
// fire-and-forget, no timer, no loop, no await added to the arm path. Every
// per-call rejection is caught HERE; the caller carries a last-resort catch,
// so an unhandled rejection is impossible either way. Field discipline: error
// CODE tokens only — never a payload, never an exception message.
import hilog from '@ohos.hilog';
import statistics from '@ohos.net.statistics';
import connection from '@ohos.net.connection';

const PROBE_DOMAIN: number = 0x2900;
const PROBE_TAG: string = 'NetBirdVpn';

// The interface under test: the name the core's TUN device registers, and the
// fixed negative-control name: deliberately NOT expected on any device, kept
// within IFNAMSIZ (Linux kernel limit of 15 name characters) so the sample
// tests "legal name, no such interface" rather than an illegal-name rejection
// (if the API answers 0 for it instead of an error, that is recorded verbatim
// — it is a finding about the API's unknown-name shape, never a proof that
// the vpn-tun value works; on a real device confirm absence against the
// interface list before reading anything into it).
const PROBE_IFACE: string = 'vpn-tun';
const PROBE_NEGATIVE_IFACE: string = 'n2h-none0';

// Same shape the shell uses elsewhere for BusinessError-like throws
// (@ohos.base BusinessError carries .code); kept local so the module stays
// import-light for the host-side fixtures.
interface BusinessFailure extends Error {
  code?: number;
}

// One interface sample. A null byte field means "no number we may print":
// the call rejected, or the resolved value failed the explicit
// finite/nonnegative check — an error is NEVER turned into 0 and a bogus
// value is NEVER printed.
class IfaceSample {
  rxBytes: number | null = null;
  txBytes: number | null = null;
  rxError: string = 'none';
  txError: string = 'none';
  startedUnixMs: number = 0;
  finishedUnixMs: number = 0;
}

function renderBytes(value: number | null): string {
  return value === null ? 'unavailable' : String(value);
}

// Error-code token only: the documented BusinessError code, 'no-code' for a
// throw without one, 'invalid-value' for a RESOLVED value that is not a
// finite nonnegative byte count. Never an exception's message text, never a
// payload.
function errorCodeOf(error: Error): string {
  const code = (error as BusinessFailure).code;
  return code === undefined || code === null ? 'no-code' : String(code);
}

async function sampleIface(requestId: string, iface: string): Promise<void> {
  const sample: IfaceSample = new IfaceSample();
  sample.startedUnixMs = Date.now();
  try {
    const rx: number = await statistics.getIfaceRxBytes(iface);
    if (Number.isFinite(rx) && rx >= 0) {
      sample.rxBytes = rx;
    } else {
      sample.rxError = 'invalid-value';
    }
  } catch (error) {
    sample.rxError = errorCodeOf(error as Error);
  }
  try {
    const tx: number = await statistics.getIfaceTxBytes(iface);
    if (Number.isFinite(tx) && tx >= 0) {
      sample.txBytes = tx;
    } else {
      sample.txError = 'invalid-value';
    }
  } catch (error) {
    sample.txError = errorCodeOf(error as Error);
  }
  sample.finishedUnixMs = Date.now();
  // Epoch-ms values ride %{public}s (never %{public}d): they exceed the
  // 32-bit int the d formatter targets, same discipline as the other
  // large-number fields in this shell.
  hilog.info(PROBE_DOMAIN, PROBE_TAG,
    'N2H_IFACE_BYTES_PROBE|requestId=%{public}s|interface=%{public}s|' +
    'rxBytes=%{public}s|txBytes=%{public}s|rxError=%{public}s|txError=%{public}s|' +
    'startedUnixMs=%{public}s|finishedUnixMs=%{public}s|diag=true|is_evidence=false',
    requestId, iface, renderBytes(sample.rxBytes), renderBytes(sample.txBytes),
    sample.rxError, sample.txError, String(sample.startedUnixMs),
    String(sample.finishedUnixMs));
}

// The whole probe: TWO interface samples (the iface under test + the negative
// control), four statistics promises total, sequential on purpose — two short
// one-shot calls, no parallelism to reason about, and the negative control
// always runs. Resolves normally in every case (each call carries its own
// catch); the caller's last-resort catch exists for the impossible path only.
export async function runInterfaceStatsProbe(requestId: string): Promise<void> {
  await sampleIface(requestId, PROBE_IFACE);
  await sampleIface(requestId, PROBE_NEGATIVE_IFACE);
}

// ---------------------------------------------------------------------------
// N2-H network-context snapshot (2026-09-20, diagnostic-only).
//
// What this is: ONE one-shot metadata read of the PUBLIC connection-module
// getters, paired with the parallel native SO_MARK observation — SDK 26.0.0.821
// @ohos.net.connection.d.ts, read locally, all Sync forms verified:
//   getDefaultNetSync(): NetHandle            (:182, since 9, GET_NETWORK_INFO)
//   getAppNetSync(): NetHandle                (:687, since 10)
//   getAllNetsSync(): Array<NetHandle>        (:230, since 10, GET_NETWORK_INFO)
//   getNetCapabilitiesSync(NetHandle)         (:349, since 10, GET_NETWORK_INFO)
//   getConnectionPropertiesSync(NetHandle)    (:289, since 10, GET_NETWORK_INFO)
//   NetHandle.netId: number (:1680; 0 = no network, valid values >= 100),
//   ConnectionProperties.interfaceName (:2106) / .routes: RouteInfo[] (:2143),
//   RouteInfo.destination: LinkAddress{address, prefixLength} (:2177/:2190),
//   .gateway: NetAddress (:2190), .hasGateway (:2195),
//   .isExcludedRoute? boolean since 20 (:2217),
//   NetCapabilities.bearerTypes: Array<NetBearType> (:1841).
//
// What this is NOT: NOT a routing verdict. A default/app/all NetHandle plus its
// interface/routes/gateway metadata is the SYSTEM's view of its networks at
// this instant — it does NOT tell which path THIS socket (or the native core's
// socket) finally takes. There is no public SO_MARK bit layout; this probe
// neither reads nor decodes SO_MARK — a matching netId is circumstantial at
// most. No socket binding, no app-network binding, no route/permission change,
// no UID statistics: getters only. No SSID/MAC/credentials field is read — the
// connection API does not expose them in the fields taken above.
//
// Field discipline (same as the iface probe): one
// `N2H_NET_CONTEXT|...|diag=true|is_evidence=false` line per arm; every API
// rejection is caught HERE and printed as the BusinessError CODE token only
// ('no-code' without one); a failed read is 'unavailable', never turned into a
// value; empty results print as 'empty'/'0' verbatim. Epoch-ms values ride
// %{public}s as strings (never %{public}d). getAllNets order is processed
// verbatim up to NETCTX_MAX_NETS entries (the excess is announced via
// truncated=true, the full count stays in nets=). The caller contract is the
// iface probe's: arm SUCCESS branch only, fire-and-forget, no timer, no loop,
// no await added to the arm path.
const NETCTX_PREFIX: string = 'N2H_NET_CONTEXT|';
// Log-line bound: a phone answers with 1-3 nets; the cap keeps one anomalous
// answer from producing an unbounded single line while the nets= field keeps
// the honest full count.
const NETCTX_MAX_NETS: number = 8;

// One RouteInfo rendered into delimiter-safe tokens ('+' between routes,
// '~' between fields — '|' and '=' stay reserved for the marker frame):
// dest~<address>~prefix~<n>~excl~<true|false|absent>~gw~<address|none>.
// 'absent' = the optional (since-20) isExcludedRoute key was not delivered;
// 'none' = hasGateway false. Values verbatim, never normalized into CIDR.
function renderNetCtxRoutes(routes: Array<connection.RouteInfo>): string {
  if (routes === undefined || routes === null) {
    return 'unavailable';
  }
  if (routes.length === 0) {
    return 'empty';
  }
  let out: string = '';
  for (let i = 0; i < routes.length; i++) {
    const route: connection.RouteInfo = routes[i];
    const dest: string = route.destination !== undefined && route.destination !== null &&
      route.destination.address !== undefined && route.destination.address !== null
      ? String(route.destination.address.address)
      : 'none';
    const prefix: string = route.destination !== undefined && route.destination !== null &&
      Number.isFinite(route.destination.prefixLength)
      ? String(route.destination.prefixLength)
      : 'invalid';
    const excl: string = route.isExcludedRoute === undefined || route.isExcludedRoute === null
      ? 'absent'
      : String(route.isExcludedRoute);
    const gw: string = route.hasGateway === true && route.gateway !== undefined &&
      route.gateway !== null && route.gateway.address !== undefined
      ? String(route.gateway.address)
      : 'none';
    out += (i > 0 ? '+' : '') + 'dest~' + dest + '~prefix~' + prefix +
      '~excl~' + excl + '~gw~' + gw;
  }
  return out;
}

// One NetHandle's metadata segment (fields joined by ','). The two per-net
// reads are independent: a rejected caps read leaves bearer unavailable with
// its code while the props read still records, and vice versa.
function renderNetCtxSegment(handle: connection.NetHandle): string {
  const idToken: string = handle === undefined || handle === null
    ? 'undefined'
    : Number.isFinite(handle.netId) ? String(handle.netId) : 'invalid';
  let bearer: string = 'unavailable';
  let capsError: string = 'none';
  let iface: string = 'unavailable';
  let propsError: string = 'none';
  let routes: string = 'unavailable';
  try {
    const caps: connection.NetCapabilities = connection.getNetCapabilitiesSync(handle);
    if (caps !== undefined && caps !== null && caps.bearerTypes !== undefined &&
      caps.bearerTypes !== null) {
      let joined: string = '';
      for (let i = 0; i < caps.bearerTypes.length; i++) {
        joined += (i > 0 ? '+' : '') + String(caps.bearerTypes[i]);
      }
      bearer = caps.bearerTypes.length === 0 ? 'empty' : joined;
    }
  } catch (error) {
    capsError = errorCodeOf(error as Error);
  }
  try {
    const props: connection.ConnectionProperties = connection.getConnectionPropertiesSync(handle);
    if (props !== undefined && props !== null) {
      iface = props.interfaceName === undefined || props.interfaceName === null
        ? 'none'
        : String(props.interfaceName);
      routes = renderNetCtxRoutes(props.routes);
    }
  } catch (error) {
    propsError = errorCodeOf(error as Error);
  }
  return 'netId=' + idToken + ',iface=' + iface + ',bearer=' + bearer +
    ',capsError=' + capsError + ',propsError=' + propsError + ',routes=' + routes;
}

// The netId token of one default/app answer: the handle object, or its absence,
// verbatim — 0 stays 0 (the API's documented "no network"), never a story.
function netCtxIdToken(handle: connection.NetHandle): string {
  if (handle === undefined || handle === null) {
    return 'undefined';
  }
  return Number.isFinite(handle.netId) ? String(handle.netId) : 'invalid';
}

// The whole snapshot: five synchronous getter reads, each in its own catch, so
// one rejected API never blanks the others (partial answers are the point).
// Fully synchronous body — the arm tick pays no await; the async shell only
// keeps the caller's .catch() contract uniform with the iface probe.
export async function runNetworkContextProbe(requestId: string): Promise<void> {
  const startedUnixMs: number = Date.now();
  let defaultNetId: string = 'unavailable';
  let defaultNetError: string = 'none';
  let appNetId: string = 'unavailable';
  let appNetError: string = 'none';
  let nets: string = 'unavailable';
  let allError: string = 'none';
  let truncated: string = 'false';
  let netsDetail: string = 'unavailable';
  try {
    defaultNetId = netCtxIdToken(connection.getDefaultNetSync());
  } catch (error) {
    defaultNetError = errorCodeOf(error as Error);
  }
  try {
    appNetId = netCtxIdToken(connection.getAppNetSync());
  } catch (error) {
    appNetError = errorCodeOf(error as Error);
  }
  try {
    const all: connection.NetHandle[] = connection.getAllNetsSync();
    if (all === undefined || all === null) {
      allError = 'invalid-value';
    } else {
      nets = String(all.length);
      if (all.length === 0) {
        netsDetail = 'empty';
      } else {
        truncated = all.length > NETCTX_MAX_NETS ? 'true' : 'false';
        const capped: number = all.length > NETCTX_MAX_NETS ? NETCTX_MAX_NETS : all.length;
        const parts: string[] = [];
        for (let i = 0; i < capped; i++) {
          parts.push(renderNetCtxSegment(all[i]));
        }
        netsDetail = parts.join(';');
      }
    }
  } catch (error) {
    allError = errorCodeOf(error as Error);
    nets = 'unavailable';
  }
  const finishedUnixMs: number = Date.now();
  hilog.info(PROBE_DOMAIN, PROBE_TAG,
    NETCTX_PREFIX + 'requestId=%{public}s|defaultNetId=%{public}s|' +
    'defaultNetError=%{public}s|appNetId=%{public}s|appNetError=%{public}s|' +
    'nets=%{public}s|allError=%{public}s|truncated=%{public}s|netsDetail=%{public}s|' +
    'startedUnixMs=%{public}s|finishedUnixMs=%{public}s|diag=true|is_evidence=false',
    requestId, defaultNetId, defaultNetError, appNetId, appNetError,
    nets, allError, truncated, netsDetail, String(startedUnixMs),
    String(finishedUnixMs));
}

// ---------------------------------------------------------------------------
// N2-H posctl probe netId resolution (2026-09-20, diagnostic-only).
//
// What this is: the netId the arm hands the core as `probe_net_id` so the
// native side can DIAGNOSTICALLY bind the ONE posctl probe socket to the VPN
// network (OH_NetConn_BindSocket, core-side) instead of leaving its egress to
// the default network — the 0021 evidence (mark=0, no SO_BINDTODEVICE, device
// TUN blind to a2b while the peer saw it via the LAN gateway address) is
// CONSISTENT with default-network egress but proves no mechanism; this
// resolution only supplies the number, the binding question stays open until
// a device run reads the answer.
//
// What this is NOT: no socket is touched here, no app-network or per-socket
// binding call, no route/permission change — the SAME official read-only
// getters the network-context snapshot already uses
// (getAllNetsSync + getConnectionPropertiesSync, GET_NETWORK_INFO, both Sync).
// The resolution runs BEFORE the native arm (the iface/NETCTX probes run
// after it, in the arm SUCCESS branch), is fully synchronous (the arm tick
// pays no await), and never throws: every API rejection is caught HERE.
//
// Result discipline: `found` carries the ONE handle whose interfaceName is
// exactly `vpn-tun` (the core's own TUN name, PROBE_IFACE). Zero hits is
// `not-found`, two or more is `ambiguous` (a name collision must not let the
// core bind a guessed network — the key is then omitted, never resolved by
// list order), a rejected list read is `list-error` with its BusinessError
// CODE token, a non-array answer is `invalid-value`, and one unread handle is
// skipped (its props rejection is not fatal for the remaining handles).
export class VpnNetIdResolution {
  found: boolean = false;
  netId: number = 0;
  reason: string = 'none';
  code: string = 'none';
}

export function resolveVpnTunNetId(): VpnNetIdResolution {
  const res: VpnNetIdResolution = new VpnNetIdResolution();
  let all: connection.NetHandle[];
  try {
    all = connection.getAllNetsSync();
  } catch (error) {
    res.reason = 'list-error';
    res.code = errorCodeOf(error as Error);
    return res;
  }
  if (all === undefined || all === null) {
    res.reason = 'invalid-value';
    return res;
  }
  let hit: number = 0;
  let hits: number = 0;
  for (let i = 0; i < all.length; i++) {
    const handle: connection.NetHandle = all[i];
    if (handle === undefined || handle === null || !Number.isFinite(handle.netId)) {
      continue;
    }
    let iface: string | null = null;
    try {
      const props: connection.ConnectionProperties = connection.getConnectionPropertiesSync(handle);
      if (props !== undefined && props !== null && props.interfaceName !== undefined &&
        props.interfaceName !== null) {
        iface = String(props.interfaceName);
      }
    } catch (error) {
      // one unread handle is skipped, not fatal for the remaining handles
      continue;
    }
    if (iface === PROBE_IFACE) {
      hits++;
      if (hits > 1) {
        res.reason = 'ambiguous';
        return res;
      }
      hit = handle.netId;
    }
  }
  if (hits === 1) {
    res.found = true;
    res.netId = hit;
    res.reason = 'found';
  } else if (hits === 0) {
    res.reason = 'not-found';
  }
  return res;
}
