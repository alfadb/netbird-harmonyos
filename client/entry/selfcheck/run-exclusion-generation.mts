// N2-H I3a generation-marker self-check (host-side, no device, no network, no
// build). Preregistration v3.1 §2.1 C2 (1)/(2)/(2a)/(4) + R-1/R-2:
//   (1)  one VPN_EXCLUSION_GENERATION line per create(), BEFORE that create(),
//        n starting at 0 and +1 with no gap within one requestId;
//   (2)  every VPN_ROUTE_ENTRY row of that generation carries the same gen=<n>;
//   (2a) the announced routes=<r>/exclusions=<m> equal the rendered row set
//        (row count, index uniquely covering 0..r-1, isExcludedRoute=="true"
//        row count);
//   (4)  the acceptance anchor: kind=initial -> VPN_CREATE_RESOLVED,
//        kind=recreate -> VPN_RECREATE_RESOLVED|…|gen=<n>|accepted=true, both
//        AFTER the generation line and its rows;
//   R-2  the arm's evidence generation G (collector.exclusion_generation /
//        N2H_COLLECTOR_ARMED|gen=G / the last accepted generation line) comes
//        from ONE source and must be literally equal.
//
// Principle of run-relay-exclusion.mts, taken to the generation bookkeeping:
// the REAL production files are copied byte-for-byte into OS-temp fixtures
// (.ts) at run time and the REAL extension class is driven END-TO-END —
//   NetBirdVpnExtensionAbility.ets (+ real siblings NetBirdConnector /
//   NetBirdVpnConfig / NetBirdRouteValidation / NetBirdEndpointExclusion);
// @ohos.* / libnetbird_core.so are functional stubs injected by
// exclusion-generation-hooks.mjs (recording hilog, fake DNS table, fake fd
// probe, fake network map, fake VpnConnection). The fixtures are never
// rewritten.
//
// Coverage (task mapping):
//   G1 initial leg: the REAL logExclusionGeneration + logRouteTable emit the
//      exact marker literal and the gen-stamped rows of the submitted config
//   G2 initial acceptance: the REAL settleCreate emits
//      VPN_CREATE_RESOLVED|accepted=true and makes that gen the accepted one
//      (generation line + all rows BEFORE the anchor)
//   G3 recreate leg: the REAL performControlledRecreate drives a full
//      destroy -> create -> fd contract -> wg feed -> ACK cycle: the recreate
//      marker (kind=recreate), its rows, and the new acceptance anchor
//      VPN_RECREATE_RESOLVED|gen=1|accepted=true, with the pre-existing
//      VPN_RECREATE_SNAPSHOT / VPN_RECREATE_DONE lines unchanged
//   G4 a SECOND recreate: gen=2, and the whole generation sequence is 0,1,2
//      (+1, no gap, kinds initial/recreate/recreate)
//   G5 three-way R-2 consistency, driven through the REAL arm hook: the arm
//      option the core echoes into collector.exclusion_generation, the
//      N2H_COLLECTOR_ARMED|gen= field and the last ACCEPTED generation line
//      all carry the same number
//   G6 fail-closed honesty: a create() that is NOT accepted never becomes the
//      accepted generation, and the arm then reports NO generation (the key is
//      omitted) instead of fabricating one
//   G7 wiring pins on the byte-identical source (the pre-create call sites,
//      the two anchor literals, the untouched existing recreate lines and the
//      arm's single-source read)
//   B1-B6 (I5b §13.16): the two PATH beacons — `stage=onCreate-reached` at the
//      extension's earliest reachable lifecycle point (BEFORE the duplicate
//      onCreate early return) and `stage=watcher-started` at the status
//      watcher's start (after its existing once-guard, outside the tick) —
//      each emit EXACTLY one line per instance with the five-field
//      `requestId|stage|diag|is_evidence` shape and nothing else; the onCreate
//      beacon survives the async start flow; the watcher beacon is not
//      re-printed by a second start call nor by three REPLAYED REAL ticks. The
//      three-state reading matrix: beacons + arm file absent ⇒ exactly one
//      ARM_SKIPPED (I5a, through the REAL tick); beacons + zero ticks ⇒ the
//      beacons and NO arm line; neither beacon ⇒ no N2H_COLLECTOR_PATH line at
//      all (extension process never started). Source pins close the group.
//   A1-A6 (I5a §13.15 + 2026-09-18 real-device timing fix): the arm hook's
//      outcome is never silent — an absent or unreadable arm control file
//      emits exactly ONE N2H_COLLECTOR_ARM_SKIPPED line (reason=arm-file-absent
//      for a PROVED absence: boolean false or BusinessError 13900002; reason=
//      arm-file-unreadable for every other error token, which does not prove
//      absence) carrying the raw code as accessError=, WITHOUT calling the core
//      arm, and that probe consumes the instance's one-shot latch (a file
//      appearing later never arms it). With the ticket PRESENT but the session
//      gate closed the attempt is NOT consumed: one deduped
//      N2H_COLLECTOR_WAITING|attempted=false line (NOT_ARMED retired — the
//      0016 run showed the first tick eating the only attempt while WG was
//      still ramping), the SAME instance then arms exactly once on the first
//      gate-open tick with the payload unchanged, a tick after that repeats
//      nothing, destroyRequested still arms nothing, and the gate-open ARMED
//      path stays byte-identical (regression).
//   C1-C6 (N2-H arm private channel, 2026-09-18): the arm ticket moved from
//      the app-invisible /data/local/tmp path into the app's OWN filesDir,
//      written by the REAL EntryAbility from the non-secret `n2h_arm_options`
//      want parameter and read by the REAL extension under the SAME basename
//      (`netbird-n2h-arm`) and the SAME filesDir mechanism as the credentials:
//      C1 the want payload lands byte-verbatim in filesDir, logged as
//      file-name + UTF-8 byte count with the payload never printed (C1b: a
//      multibyte payload pins the UTF-8 count against UTF-16 impersonation);
//      C2 the
//      extension's probe + read resolve <filesDir>/netbird-n2h-arm and the
//      EXISTING arm options (posctl_port / probe_wait_ms /
//      peer_b2a_probe_id) reach the core n2h_collector_arm unchanged; C3 a
//      launch WITHOUT the parameter deletes a leftover ticket (default: no
//      arm) while every credential/config/deviation file stays byte-identical;
//      C4 an invalid-JSON payload keeps the ORIGINAL convention
//      (ARM_OPTS_IGNORED + §5.1 defaults, honestly logged, still no fabricated
//      success); C5/C5b a failed OPEN and a failed/short WRITE after the
//      TRUNC-ing open each leave NO usable ticket (the latter removes exactly
//      the wiped diagnostic file, logged independently with codes) and the
//      extension then reports arm-file-absent (never a faked arm); C5c a
//      failed CLEANUP is logged as REMOVE_FAILED with its code — never claimed
//      as removed; C6 the per-instance
//      one-shot arm attempt is intact on the new channel (a second tick calls
//      the core again ZERO times). Source pins: the extension no longer
//      references /data/local/tmp/netbird-n2h-arm anywhere, the credential
//      dev-override channel is untouched.
//   C8 (2026-09-18 peer_overlay_ip bridge): the parallel posctl-target fix's
//      optional non-secret `peer_overlay_ip` (the ACTUAL n2h-peer overlay
//      address; multi-peer maps no longer mean first()) is bridged with the
//      peer_b2a_probe_id verbatim discipline — present ⇒ in the native opts
//      JSON verbatim (existing fields unchanged), absent ⇒ the key stays
//      absent (no default injected), wrong-type (e.g. a JSON number) ⇒
//      forwarded AS THAT VALUE for the core's bare-IPv4 + peer-map check to
//      refuse; no marker field and no log literal changes.
//   C9 (2026-09-20 dns_nxdomain_echo bridge): v4 effective preregistration
//      §3 path① / §6.2 item 2's optional non-secret `dns_nxdomain_echo`
//      (the explicit enable of the dns-nxdomain-echo receipt method; default
//      OFF) is bridged with the SAME present-only + verbatim discipline —
//      present true/false ⇒ the key lands in the native opts JSON verbatim
//      (existing fields unchanged), absent ⇒ the key stays ABSENT (no `false`
//      default injected), wrong-type (JSON string/number) ⇒ forwarded AS THAT
//      VALUE for the core's strict boolean check to refuse; no marker field
//      and no log literal changes.
//   P1-P8 (2026-09-18 iface-bytes availability probe): the OFFICIAL
//      @ohos.net.statistics getIfaceRxBytes/getIfaceTxBytes (since 10, no
//      permission on the iface methods) is called ONLY from the arm SUCCESS
//      branch, exactly once per instance, as one four-promise round over
//      'vpn-tun' + the fixed negative-control name 'n2h-none0'.
//      P1 the four calls (negative control actually invoked); P2 the vpn-tun
//      line carries the resolved byte values, none-errors, digit-shaped
//      begin/end ms and the diag flags; P3 the unknown name's 0-answer is
//      recorded verbatim (never read as a proof about vpn-tun); P4 a
//      per-direction rejection leaves that byte field unavailable with the
//      raw code kept while the other direction records; P5 NO native ARM
//      success ⇒ NO probe (no ticket / gate closed / core answers not-ok);
//      P6 ticks after the arm repeat nothing (one-shot inherited) — the FIRST
//      post-arm tick carries exactly the WINDOW close edge, the tick after it
//      repeats nothing; P7 a
//      resolved non-finite/negative value is never printed and never turned
//      into 0 (invalid-value), a resolved 0 prints as 0; P8 source pins (call
//      site inside the ok-branch after the ARMED literal, byte-only probe
//      module — no /proc read, no getUid*/GET_NETWORK_STATS, no exception
//      message text — and module.json5 unchanged). The probe never touches
//      Evidence.counters/notes; no packet count exists anywhere.
//   W1-W7 (2026-09-20 iface-bytes WINDOW two-edge read, v4 preregistration
//      §4 (ii) C7 same-source-calibration prerequisite): the OPEN edge is the
//      arm SUCCESS branch's own vpn-tun reading, re-rendered on
//      N2H_IFACE_BYTES_WINDOW|phase=open from the SAME numbers (no second
//      read; pairRequestId == requestId); the CLOSE edge is ONE more vpn-tun
//      read issued on the FIRST watcher TRIGGER AFTER that open line — never
//      the arming tick (the latch flips only after the async open line),
//      never without an open edge, never twice (latch consumed at issue) —
//      and, since the 2026-09-20 fix, issued at the VERY TOP of the tick so
//      NO early-return form bypasses it (v4 §4/§6.2 mechanical two-edge
//      requirement: open/close each EXACTLY once holds in EVERY tick form).
//      W1 open edge once + zero close before the next tick; W2 the close
//      edge: same requestId, fresh values, close.started >= open.finished,
//      exactly the two vpn-tun calls; W3 later ticks repeat nothing; W4 a
//      rejected API prints unavailable + the code and the flow continues;
//      W5 no ARM success (no ticket / gate closed / core not-ok) ⇒ zero
//      close reads; W6 source pins (no timer/loop in the window path, no
//      await added to the tick, the close hook rides the watcher tick BEFORE
//      the status read / terminal / recreate early returns and no longer
//      inside the arm hook, the read issues only behind the open latch with a
//      last-resort catch, latch callback writes one boolean inside the
//      ok-branch, both reads strictly after the native arm call); W7 the
//      terminal / recreate / status-unreachable tick forms still issue the
//      close edge EXACTLY once (the 2026-09-20 fix, replayed REAL ticks).
//      C7 counting input only — never Evidence.
//   NC1-NC5 (2026-09-20 network-context snapshot): the PUBLIC connection
//      getters (getDefaultNetSync/getAppNetSync/getAllNetsSync +
//      getNetCapabilitiesSync/getConnectionPropertiesSync, all Sync forms read
//      off the local SDK d.ts) are sampled ONCE per arm SUCCESS, in the same
//      branch as the iface probe. NC1 the armed line carries the seeded
//      default/app netIds, the getAllNets list verbatim and per-net
//      interface/bearerTypes/routes (dest/prefix/excl/gw tokens) with digit
//      begin/end; NC2 a rejected API (code) leaves that field unavailable
//      while every independent read still records; NC3 an empty handle world
//      records nets=0/detail=empty honestly; NC4 no ARM success (gate closed /
//      core not-ok) fires nothing and the armed instance never repeats; NC5
//      source pins (getter-only module — no bindSocket/setAppNet, no
//      .message, still byte-only; call order inside the ok-branch; aborted
//      guard literal).
//   IB1-IB8 (2026-09-26 iface-bytes BURST, the C7 same-source calibration's
//      survey half — n2h-kit/c7-same-source-plan-20260926.md §3 mechanism a):
//      the arm ticket's DEFAULT-OFF `diag_iface_bytes_burst` switch starts a
//      bounded 500 ms series of vpn-tun reads in the arm SUCCESS branch, one
//      `N2H_IFACE_BYTES_BURST|requestId=…|seq=…|interface=vpn-tun|rxBytes=…|
//      txBytes=…|rxError=…|txError=…|startedUnixMs=…|finishedUnixMs=…|
//      diag=true|is_evidence=false` line per tick (a NEW prefix: the window and
//      probe line counts stay exactly one). IB1 the key ABSENT (the default):
//      the arm still arms, with ZERO burst lines and ZERO intervals; IB2 ON
//      (literal true): exactly ONE 500 ms interval, seq from 0, the fixed
//      field order and the two vpn-tun calls per tick (never the negative
//      control), the key never reaching the core arm options, and no
//      PROBE/WINDOW line from a burst tick; IB3 the hard cap (40 = 20 s, past
//      the ≥10 s gap the C7 pairing needs): seq 0..39 then the interval is
//      cleared and a further callback emits nothing; IB4 the window CLOSE edge
//      does NOT clear it — the series keeps emitting past that edge and only
//      the cap stops it; IB5 stopStatusAndNetcfgWatcher clears it immediately;
//      IB6 onDestroy clears it immediately; IB7 the shared readDiagSwitch type
//      discipline (string/number/null ⇒ OFF plus the ONE ignored-key line,
//      literal false ⇒ OFF and silent); IB8
//      source pins (one shared switch read, the start inside the ok-branch
//      after the ARMED literal behind its own gate, the 500 ms/40 constants,
//      the TWO stop call sites with NO stop at the close edge, the new prefix
//      and the timer-free probe module).
//
//   CR1-CR7 (2026-09-26 S6, protect-bypass plan §3.1 + §6 item 1②): the
//      `diag_capture_route` arm-ticket switch. CR1 pins the wiring (ONE shared
//      readDiagSwitch key + the target read off the ticket's own
//      protect_probe object, TWO gated call sites in the tunnel-subnet entry's
//      placement); CR2 the default (switch off, target present ⇒ the submitted
//      VpnConfig is byte-identical and nothing is logged); CR3/CR4 the added
//      non-excluded managed /32 on the initial AND recreate legs (appended after
//      the tunnel-subnet entry, covered by the generation marker / route rows /
//      validation gate, and the switch provably absent from the arm docs);
//      CR5 an EXCLUDED target (a frozen endpoint /32) is refused with
//      reason=target-excluded instead of shadowing the exclusion; CR6 the pure
//      helper's strict `IPv4:port` mirror of the core's parse (skip tokens
//      no-target / invalid-target / target-excluded, /24 containment, IPv6
//      exclusions and non-excluded routes not counted, input never mutated);
//      CR7 the switch on with no usable target.
//   PP0-PP9 (2026-09-26 S7, protect-bypass plan §4/§6 item 5): the protect-probe
//      fd hand-off. PP0 the verbatim passthrough of the two new arm keys
//      (`posctl_probes` / `protect_probe`) with the pre-existing key set
//      otherwise unchanged, wrong-typed values forwarded AS THEY ARE; PP1
//      mode:none and an absent key are the ZERO-hand-off cases (no open, no
//      protect, no commit, no line) with the arm untouched; PP2 the order
//      open -> protect(fd) -> commit(fd,protected) -> arm on the fd open
//      answered, plus the fixed line literal; PP3 the route-exclusion protect
//      DEVIATION still calls the platform; PP4/PP5 rejected -> unprotected and
//      synchronous throw -> error; PP6/PP6b the bounded 5 s box with a captured
//      timer (timeout -> error with nothing re-armed or left behind; a settled
//      call CLEARS the handle); PP7/PP7b a refused/unparseable open prints its
//      own line (codes/names only) and the arm STILL runs; PP8 a refused commit
//      lands verbatim in `commit=`; PP9 the fd-ownership pins (the hand-off's
//      three methods call ONLY n2h_probe_socket_open/commit and carry no
//      close/dup/fcntl on the fd — the core stays the ONE owner).
//
// Run: node client/entry/selfcheck/run-exclusion-generation.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./exclusion-generation-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
// The fixture EntryAbility lives in its own source dir (arm private channel,
// C group): staged byte-identically like the five extension modules.
const REAL_ENTRY_DIR = join(HERE, '..', 'src', 'main', 'ets', 'entryability');

// The filesDir every arm-driven fixture instance shares — the string the B
// group already used; the fake fs (exclusion-generation-hooks.mjs) keys its
// in-memory map by the RESOLVED path, so the extension's real
// `<context.filesDir>/netbird-n2h-arm` join and the fixture EntryAbility's
// own writes land on one and the same file.
const FIXTURE_FILESDIR = '/tmp/n2h-i5b-fixture';
const ARM_TICKET_PATH = `${FIXTURE_FILESDIR}/netbird-n2h-arm`;
const ARM_TICKET_NAME = 'netbird-n2h-arm';
// The 2026-09-18 iface-bytes availability probe marker (the P group below).
const PROBE_PREFIX = 'N2H_IFACE_BYTES_PROBE|';
// The 2026-09-20 iface-bytes WINDOW two-edge marker (the W group below).
const WINDOW_PREFIX = 'N2H_IFACE_BYTES_WINDOW|';
// The 2026-09-26 iface-bytes BURST marker (the IB group below) — a NEW prefix
// on purpose: the W/P assertions pin the window/probe prefixes at an exact
// line count, so the 500 ms survey series must never reuse them.
const BURST_PREFIX = 'N2H_IFACE_BYTES_BURST|';
// The 2026-09-20 network-context snapshot marker (the NC group below).
const NETCTX_LINE = 'N2H_NET_CONTEXT|';

// The five real modules staged byte-identically (code under test = shipped
// code), and the erased interface names each shim must re-declare for the ESM
// link (same list as run-relay-exclusion.mts — see that file's hook comment).
// NetBirdInterfaceStatsProbe (2026-09-18 iface-bytes availability probe) is
// staged the same way: the REAL extension imports it, so the byte-identical
// fixture needs the byte-identical helper (no sibling imports of its own —
// no shim needed).
const STAGED = ['NetBirdVpnExtensionAbility', 'NetBirdConnector',
  'NetBirdVpnConfig', 'NetBirdRouteValidation', 'NetBirdEndpointExclusion',
  'NetBirdInterfaceStatsProbe', 'NetBirdPhysicalNetworkPolicy',
  // N5c demand-sized ICE pool (2026-10-02): the sizing pure module the REAL
  // extension imports; zero imports and no erased types, so no shim.
  'NetBirdIceSocketPool',
  // UI phase 1 (2026-10-05): the status-document builder pure module the REAL
  // extension imports; its two erased interfaces are re-declared by its shim.
  'NetBirdUiModel',
  // roam-1005-0014: the recreate lifecycle policy pure module the REAL
  // extension imports; zero imports and no erased types, so no shim.
  'NetBirdRecreatePolicy',
  // N2 T5a (2026-09-28): the stimulus pure module the REAL extension imports;
  // its two erased interfaces are re-declared by its shim below.
  'NetBirdN2Stimulus',
  // N2 P4/P5 (2026-09-28): the A-column trigger and sandwich pure modules the
  // REAL extension imports; their erased interfaces are re-declared by shims.
  'NetBirdN2AEvent',
  'NetBirdN2Sandwich',
  // N2 T3 §3 (2026-09-28): the positive-control request pure module the REAL
  // extension imports; its two erased interfaces are re-declared by its shim.
  'NetBirdN2ControlRequest',
  // N2 first batch C/D columns (2026-09-29): the switching-sandwich pure module
  // (plan parse + bearer classification + direction verdict + target choice)
  // and the ring-dump pure module the REAL extension imports.
  'NetBirdN2CdPlan',
  'NetBirdN2Ring'];
const SHIM_TYPES: Record<string, string[]> = {
  NetBirdConnector: ['ConnectorStatusResult', 'CoreNetworkConfig', 'CoreRouteEntry',
    'ConnectorStartResult', 'ConnectorLogoutResult', 'DeviceConfigFile', 'ManagementEndpoint',
    'NativeTcpSocketOpen', 'RelayAdvertisedView', 'RouteSetAppliedResult',
    'SocketFeedResult', 'SocketSupplyRequest', 'TunReleaseResult', 'WgFeedResult',
    // N2 first batch C/D columns: the erased bearer-facts shape
    // `physicalDefaultBearerFacts` answers.
    'PhysicalBearerFacts',
    // 2026-10-08 diagnostics wiring: the erased phone_log result/summary shapes
    // the real extension now imports alongside the four phoneLog* functions.
    'PhoneLogConfigureResult', 'PhoneLogSummary'],
  // UI phase 1: the erased input/document shapes buildUiStatusDocument names.
  NetBirdUiModel: ['UiNetworkConfig', 'UiStatusDocument'],
  NetBirdEndpointExclusion: ['ExcludedRouteEntry', 'EndpointResolution',
    'ExclusionBuildResult', 'ExclusionEndpoint', 'RelayEndpoint',
    'DnsDerivationInput', 'DnsDerivationResult', 'DnsNetProbe',
    'DnsPlatformNetAddress',
    'StunDerivationResult', 'StunUriEntry', 'StunParseResult', 'StunEndpoint'],
  NetBirdVpnConfig: ['VpnRouteConfig', 'VpnTunnelConfig',
    // S1 diag switch (2026-09-23): the erased outcome interface the real
    // extension imports alongside withTunnelSubnetRoute
    'TunnelSubnetRouteOutcome',
    // S6 diag capture route (2026-09-26): the second erased outcome interface
    // the real extension now imports alongside withCaptureRoute
    'CaptureRouteOutcome'],
  NetBirdRouteValidation: ['RouteCheckResult', 'RouteViolation'],
  // N2 T5a: the erased stimulus request/reply shapes the extension names.
  NetBirdN2Stimulus: ['N2StimulusDecision', 'N2StimulusReply'],
  // N2 P4/P5: the erased trigger and sandwich shapes the extension names.
  NetBirdN2AEvent: ['N2AEventDecision', 'N2AEventRequest', 'N2AEventReply'],
  NetBirdN2Sandwich: ['N2SandwichPost'],
  // N2 T3 §3: the erased positive-control request/decision shapes.
  NetBirdN2ControlRequest: ['N2ControlDecision', 'N2ControlRequest'],
  // N2 first batch C/D columns (2026-09-29): the erased shapes of the
  // switching-sandwich and ring-dump pure modules.
  NetBirdN2CdPlan: ['N2CdPlanDecision', 'N2CdPlanRequest', 'N2BearerFacts'],
  NetBirdN2Ring: ['N2RingDumpDecision', 'N2RingDumpRequest',
    // 2026-09-29 (explicit host ACK): the erased ACK request/decision shapes.
    'N2RingAckDecision', 'N2RingAckRequest']
};

const REQUEST_ID = 'req-i3a';

let passed: number = 0;
let failedCount: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log('ok   ' + name);
  } else {
    failedCount++;
    console.log('FAIL ' + name + (detail !== undefined ? ' — ' + detail : ''));
  }
}

const G: Record<string, unknown> = globalThis as unknown as Record<string, unknown>;

function logs(): string[] {
  return (G['__NB_LOG'] as string[]) ?? [];
}

function resetLog(): void {
  G['__NB_LOG'] = [];
}

function linesWith(prefix: string): string[] {
  // The recording stub prefixes each line with '[<lvl>] ' — strip it so the
  // assertions match the REAL hilog format strings verbatim.
  return logs().filter((l: string): boolean => l.indexOf(prefix) >= 0)
    .map((l: string): string => l.slice(l.indexOf(prefix)));
}

function lastLine(prefix: string): string {
  const hits = linesWith(prefix);
  return hits.length === 0 ? '' : hits[hits.length - 1];
}

// One `key=value` field of a pipe-delimited marker line ('' when absent).
function fieldOf(line: string, key: string): string {
  const hit = line.split('|').find((f: string): boolean => f.indexOf(key + '=') === 0);
  return hit === undefined ? '' : hit.slice(key.length + 1);
}

function freshAbility(mod: Record<string, unknown>,
  filesDir?: string): Record<string, unknown> {
  const ability = new (mod['default'] as new () => Record<string, unknown>)();
  ability['requestId'] = REQUEST_ID;
  // Arm-driven instances need the context the REAL lifecycle always provides;
  // the B-group lifecycle beacons deliberately keep the context ABSENT (their
  // "async start flow aborts on the fixture's absent context" regression).
  if (filesDir !== undefined) {
    ability['context'] = { filesDir: filesDir };
  }
  return ability;
}

// The live-session status the REAL arm gate reads (wg.device_up && wg.ready).
const WG_READY: Record<string, unknown> = { wg: { device_up: true, ready: true } };

// One watcher tick of the REAL arm hook.
function tickArm(ability: Record<string, unknown>, status: Record<string, unknown>): void {
  (ability['maybeArmN2hCollector'] as (s: unknown) => void)(status);
}

// The REAL watcher tick body, replayed by hand: starts the REAL watcher with
// the interval CAPTURED (the B-group mechanism — no real timer is left
// pending) and returns the captured tick closure. The close edge rides THIS
// body's very top (before the status read and every early return), so the
// window groups replay REAL ticks, not the arm hook alone.
function watcherTick(ability: Record<string, unknown>): () => void {
  const captured: Array<() => void> = [];
  const previous = G['setInterval'];
  G['setInterval'] = (handler: () => void): number => {
    captured.push(handler);
    return 7300 + captured.length;
  };
  (ability['startStatusAndNetcfgWatcher'] as () => void)();
  G['setInterval'] = previous;
  return captured[captured.length - 1];
}

// The arm-file state of the path-aware fs stub (exclusion-generation-hooks.mjs),
// in one place: the ticket is a REAL entry in the in-memory map keyed by the
// RESOLVED path (present=true puts `text` there, present=false removes it,
// present=undefined leaves the map untouched for a pre-seeded body), and
// accessError simulates the code an accessSync throw carries ('no-code' = a
// throw without a code). No file + no error = the boolean-false answer the
// 2026-09-18 device actually produced (accessError=none).
function setArmFile(present: boolean | undefined, accessError: unknown,
  text: string = ''): void {
  const files = (G['__NB_FS'] ?? {}) as Record<string, string>;
  const errs = (G['__NB_FS_ACCESS_ERROR'] ?? {}) as Record<string, unknown>;
  if (present === true) {
    files[ARM_TICKET_PATH] = text;
  } else if (present === false) {
    delete files[ARM_TICKET_PATH];
  }
  if (accessError === undefined) {
    delete errs[ARM_TICKET_PATH];
  } else {
    errs[ARM_TICKET_PATH] = accessError;
  }
  G['__NB_FS'] = files;
  G['__NB_FS_ACCESS_ERROR'] = errs;
  G['__NB_ARM_OPTS'] = undefined;
}

function seedFile(path: string, text: string): void {
  const files = (G['__NB_FS'] ?? {}) as Record<string, string>;
  files[path] = text;
  G['__NB_FS'] = files;
}

function fileOf(path: string): string | undefined {
  return ((G['__NB_FS'] ?? {}) as Record<string, string>)[path];
}

// The statistics stub's append-only call record ('method:nic' strings) and the
// per-NIC answer tables (exclusion-generation-hooks.mjs): a number resolves,
// {code:n} rejects, an UNSET name rejects 2100003 — an unexpected probe can
// never fake a value.
function statsCalls(): string[] {
  return (G['__NB_STATS_CALLS'] ?? []) as string[];
}

function seedStats(rx: Record<string, unknown>, tx: Record<string, unknown>): void {
  G['__NB_STATS_RX'] = rx;
  G['__NB_STATS_TX'] = tx;
}

// The network-context probe's connection tables (exclusion-generation-hooks.mjs):
// a number resolves, {code:n} rejects, an UNSET key answers the historical
// default (netId 0 / empty list / wlan0 without routes) — seeding one table
// never perturbs what the older groups observe. __NB_NET_THROW_CODES maps a
// method name to a BusinessError code (or 'no-code').
function seedNetCtx(tables: Record<string, unknown>): void {
  G['__NB_NET_DEFAULT_ID'] = undefined;
  G['__NB_NET_APP_ID'] = undefined;
  G['__NB_NET_ALL_IDS'] = undefined;
  G['__NB_NET_CAPS_BY_ID'] = undefined;
  G['__NB_NET_PROPS_BY_ID'] = undefined;
  G['__NB_NET_THROW_CODES'] = undefined;
  for (const key of Object.keys(tables)) {
    G[key] = tables[key];
  }
}

// Every probe log line is written several microtasks after the tick that
// armed it (four awaited statistics promises); one macrotask boundary drains
// the whole chain deterministically (the same pattern the B1 await uses).
async function drainProbe(): Promise<void> {
  await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 0); });
}

// The desired-route-set snapshot the REAL recreate leg consumes: two managed
// routes (one of them the default route the gate releases) — the exclusion
// entries come from the frozen set grafted by the leg itself.
function netcfgJson(): string {
  return JSON.stringify({
    available: true,
    serial: 9,
    address: '10.99.0.1',
    address_prefix_len: 32,
    routes: [
      { network: '172.16.0.0/12', is_default: false },
      { network: '0.0.0.0/0', is_default: true }
    ],
    dns: { service_enable: true, servers: [{ ip: '8.8.8.8', port: 53 }] },
    default_route: { allowed: true, reason: 'wg-tunnel-ready' },
    stuns: []
  });
}

// Drive one full recreate leg through the REAL performControlledRecreate.
// Per-leg counters are reset so every assertion below is about THIS leg.
// Since 2026-09-23 the snapshot is a parameter (default: the historical
// netcfgJson above) so the S1/S2 groups can drive a /16 tunnel address and the
// historical /32 shape from the SAME code path; the pre-existing G calls are
// unchanged.
async function driveRecreate(ability: Record<string, unknown>,
  netcfgText: string = netcfgJson()): Promise<void> {
  G['__NB_NETCFG'] = netcfgText;
  G['__NB_FD_STATUS'] = { open: true, errno: 0 };
  G['__NB_CREATED_CONFIGS'] = [];
  G['__NB_OLD_DESTROYED'] = 0;
  // the OLD generation's connection (its destroy() is the recreate's step 2)
  ability['connection'] = {
    destroy: (): Promise<void> => {
      G['__NB_OLD_DESTROYED'] = (G['__NB_OLD_DESTROYED'] as number) + 1;
      return Promise.resolve();
    }
  };
  ability['createSettled'] = true;
  ability['tunFd'] = 42;
  // the data-plane feeds are preconditions of step 5, not of steps 1-4
  ability['outboundSocketFd'] = 7;
  ability['outboundProtected'] = true;
  await (ability['performControlledRecreate'] as (reason: string) => Promise<void>)(
    'desired-route-set-change');
}

try {
  const dir = mkdtempSync(join(tmpdir(), 'nb-exclusion-generation-'));
  for (const name of STAGED) {
    writeFileSync(join(dir, name + '.ts'),
      readFileSync(join(REAL_DIR, name + '.ets'))); // byte-identical real file
  }
  // The fixture EntryAbility (arm private channel, C group): byte-identical
  // real file; its diagnostics/system-event sibling imports are bridged below
  // (they are not part of route-exclusion generation).
  writeFileSync(join(dir, 'EntryAbility.ts'),
    readFileSync(join(REAL_ENTRY_DIR, 'EntryAbility.ets')));
  for (const name of Object.keys(SHIM_TYPES)) {
    writeFileSync(join(dir, name + '.shim.ts'),
      'export * from \'./' + name + '.ts\';\n' +
      SHIM_TYPES[name].map((t: string): string => 'export const ' + t + ' = undefined;')
        .join('\n') + '\n');
  }
  // Diagnostics wiring (2026-10-08): the REAL extension and the REAL
  // EntryAbility now import the diagnostics writer (start/event/stop, plus
  // appendSystemEvent for the UI's watcher sink) and the system-event watcher
  // (start/stop). Neither takes any part in route-exclusion generation, and
  // both need platform IO (hidebug / hiAppEvent) that this host fixture cannot
  // run, so each specifier is bridged by a no-op module carrying the same named
  // exports the real files import by value (their type-only imports are erased
  // by node's type stripping). The two modules' own behaviour is host-checked
  // by run-diagnostics.mts / run-system-events.mts, never here.
  writeFileSync(join(dir, 'NetBirdDiagnostics.shim.ts'),
    'export const start = () => undefined;\n' +
    'export const event = () => false;\n' +
    'export const appendSystemEvent = () => false;\n' +
    'export const stop = () => undefined;\n');
  writeFileSync(join(dir, 'NetBirdSystemEvents.shim.ts'),
    'export const start = () => false;\n' +
    'export const stop = () => undefined;\n' +
    'export const isRunning = () => false;\n');

  const mod = await import(pathToFileURL(join(dir, 'NetBirdVpnExtensionAbility.ts')).href) as
    Record<string, unknown>;
  const cfg = await import(pathToFileURL(join(dir, 'NetBirdVpnConfig.ts')).href) as
    Record<string, unknown>;

  // The merged config of the INITIAL generation, built by the REAL mapper:
  // base template + two frozen endpoint /32 exclusions, the core snapshot
  // applied, default route HELD (the evidence-round discipline). 3 rows, 2 of
  // them excluded (the base template carries NO static LAN exclusion anymore
  // — that piece is derived per create from the physical default network,
  // and this composition has no network world, so none is grafted).
  const derived = [
    { destination: '203.0.113.7', prefixLength: 32, gateway: '10.99.0.1',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: true },
    { destination: '198.51.100.4', prefixLength: 32, gateway: '10.99.0.1',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }
  ];
  const withExcl = (cfg['withEndpointExclusions'] as
    (base: unknown, e: unknown[]) => Record<string, unknown>)(
    cfg['loadVpnTunnelConfig'](), derived);
  const merged = (cfg['applyNetworkConfig'] as
    (base: unknown, snap: unknown) => Record<string, unknown>)(
    withExcl, JSON.parse(netcfgJson()) as never);
  const initialCfg = (cfg['gateDefaultRoutes'] as
    (c: unknown, allowed: boolean) => Record<string, unknown>)(merged, false);

  // ---------------- G1: the initial generation marker + its rows -----------
  const ability = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  const gen0 = (ability['logExclusionGeneration'] as
    (kind: string, routes: unknown[], k: number) => number)(
    'initial', initialCfg['routes'] as unknown[], 0);
  (ability['logRouteTable'] as (c: unknown, gen: number) => void)(initialCfg, gen0);
  const marker0 = linesWith('VPN_EXCLUSION_GENERATION|');
  check('G1 exactly ONE generation marker per create()',
    marker0.length === 1 && gen0 === 0, JSON.stringify(marker0));
  check('G1 marker literal (requestId|gen|kind|exclusions|wgPeerEndpoints|routes)',
    marker0[0] === 'VPN_EXCLUSION_GENERATION|requestId=' + REQUEST_ID +
    '|gen=0|kind=initial|exclusions=2|wgPeerEndpoints=0|routes=3', marker0.join('\n'));
  const rows0 = linesWith('VPN_ROUTE_ENTRY|');
  check('G1 row count == the announced routes= (§2.1 (2a)①)',
    rows0.length === 3 && String(rows0.length) === fieldOf(marker0[0], 'routes'),
    JSON.stringify(rows0));
  check('G1 index uniquely covers 0..r-1 (§2.1 (2a)②)',
    JSON.stringify(rows0.map((l: string): string => fieldOf(l, 'index'))) ===
    JSON.stringify(['0', '1', '2']), JSON.stringify(rows0));
  check('G1 excluded rows == the announced exclusions= (§2.1 (2a)③)',
    rows0.filter((l: string): boolean => fieldOf(l, 'isExcludedRoute') === 'true').length === 2 &&
    String(rows0.filter((l: string): boolean =>
      fieldOf(l, 'isExcludedRoute') === 'true').length) === fieldOf(marker0[0], 'exclusions'),
    JSON.stringify(rows0));
  check('G1 every row carries the marker\'s gen (§2.1 (2))',
    rows0.every((l: string): boolean => fieldOf(l, 'gen') === '0'), JSON.stringify(rows0));
  check('G1 the initial generation holds no default route (§2.1 (6) precondition)',
    rows0.every((l: string): boolean => fieldOf(l, 'isDefaultRoute') !== 'true') &&
    fieldOf(marker0[0], 'kind') === 'initial', JSON.stringify(rows0));

  // ---------------- G2: the initial acceptance anchor ----------------------
  G['__NB_FD_STATUS'] = { open: true, errno: 0 };
  const outcome = await (ability['settleCreate'] as
    (fd: number, gen: number) => Promise<Record<string, unknown>>)(42, gen0);
  const anchor0 = lastLine('VPN_CREATE_RESOLVED|');
  check('G2 initial acceptance anchor is the existing VPN_CREATE_RESOLVED|accepted=true',
    anchor0 === 'VPN_CREATE_RESOLVED|requestId=' + REQUEST_ID +
    '|fd=42|accepted=true|marker=CREATE_ACCEPTED' && outcome['accepted'] === true,
    anchor0 + ' ' + JSON.stringify(outcome));
  check('G2 the anchor makes THAT generation the accepted one (single source)',
    ability['n2hAcceptedExclusionGeneration'] === 0 &&
    ability['n2hExclusionGeneration'] === 0,
    String(ability['n2hAcceptedExclusionGeneration']));
  const logText = logs().join('\n');
  check('G2 marker + all rows precede the anchor (§2.1 (4) ordering)',
    logText.indexOf(marker0[0]) >= 0 &&
    logText.indexOf(marker0[0]) < logText.indexOf(rows0[rows0.length - 1]) &&
    logText.indexOf(rows0[rows0.length - 1]) < logText.indexOf(anchor0),
    JSON.stringify(logs()));

  // ---------------- G3: a full recreate leg through the REAL code ----------
  // the frozen exclusion set the start flow retains (this is what the recreate
  // leg grafts onto the rebuilt config)
  ability['frozenEndpointExclusions'] = derived;
  const connectionsBefore = Number(G['__NB_CONNECTIONS'] ?? 0);
  resetLog();
  await driveRecreate(ability);
  const marker1 = linesWith('VPN_EXCLUSION_GENERATION|');
  check('G3 the recreate leg announces exactly one generation, kind=recreate',
    marker1.length === 1 && fieldOf(marker1[0], 'kind') === 'recreate' &&
    fieldOf(marker1[0], 'gen') === '1', marker1.join('\n'));
  check('G3 recreate marker counts match its OWN submitted config (4 rows / 2 excluded)',
    marker1[0] === 'VPN_EXCLUSION_GENERATION|requestId=' + REQUEST_ID +
    '|gen=1|kind=recreate|exclusions=2|wgPeerEndpoints=0|routes=4', marker1.join('\n'));
  const rows1 = linesWith('VPN_ROUTE_ENTRY|');
  check('G3 recreate rows: count/index/excluded match the marker, all gen=1',
    rows1.length === 4 &&
    JSON.stringify(rows1.map((l: string): string => fieldOf(l, 'index'))) ===
    JSON.stringify(['0', '1', '2', '3']) &&
    rows1.filter((l: string): boolean => fieldOf(l, 'isExcludedRoute') === 'true').length === 2 &&
    rows1.every((l: string): boolean => fieldOf(l, 'gen') === '1'), JSON.stringify(rows1));
  const recreateAnchor = lastLine('VPN_RECREATE_RESOLVED|');
  check('G3 recreate acceptance anchor literal (§2.1 (4))',
    recreateAnchor === 'VPN_RECREATE_RESOLVED|requestId=' + REQUEST_ID +
    '|gen=1|accepted=true', recreateAnchor);
  check('G3 the recreate anchor makes gen 1 the accepted generation',
    ability['n2hAcceptedExclusionGeneration'] === 1 &&
    ability['n2hExclusionGeneration'] === 1,
    String(ability['n2hAcceptedExclusionGeneration']));
  const recreateLog = logs().join('\n');
  check('G3 generation marker + all rows precede the recreate anchor',
    recreateLog.indexOf(marker1[0]) < recreateLog.indexOf(rows1[rows1.length - 1]) &&
    recreateLog.indexOf(rows1[rows1.length - 1]) < recreateLog.indexOf(recreateAnchor),
    JSON.stringify(logs()));
  check('G3 the pre-existing recreate lines coexist unchanged',
    linesWith('VPN_RECREATE_SNAPSHOT|').length === 1 &&
    fieldOf(lastLine('VPN_RECREATE_SNAPSHOT|'), 'routes') === '4' &&
    fieldOf(lastLine('VPN_RECREATE_SNAPSHOT|'), 'frozenEndpointExclusions') === '2' &&
    fieldOf(lastLine('VPN_RECREATE_DONE|'), 'routes') === '4' &&
    fieldOf(lastLine('VPN_RECREATE_DONE|'), 'defaultRoutes') === '1',
    linesWith('VPN_RECREATE_SNAPSHOT|').join('\n') + '\n' +
    linesWith('VPN_RECREATE_DONE|').join('\n'));
  check('G3 the leg really ran to completion (fresh connection, destroy, ACK isRecreate=true)',
    (G['__NB_CREATED_CONFIGS'] as unknown[]).length === 1 &&
    G['__NB_OLD_DESTROYED'] === 1 &&
    Number(G['__NB_CONNECTIONS'] ?? 0) === connectionsBefore + 1 &&
    (G['__NB_ACK'] as Record<string, unknown>)['isRecreate'] === true,
    JSON.stringify({ created: (G['__NB_CREATED_CONFIGS'] as unknown[]).length,
      destroyed: G['__NB_OLD_DESTROYED'], connections: G['__NB_CONNECTIONS'],
      ack: G['__NB_ACK'] }));

  // ---------------- G4: a SECOND recreate -> gen=2, sequence 0,1,2 ---------
  const connectionsBefore2 = Number(G['__NB_CONNECTIONS'] ?? 0);
  resetLog();
  await driveRecreate(ability);
  const marker2 = linesWith('VPN_EXCLUSION_GENERATION|');
  check('G4 the second recreate is generation 2 (monotonic +1, no gap)',
    marker2.length === 1 && fieldOf(marker2[0], 'gen') === '2' &&
    fieldOf(marker2[0], 'kind') === 'recreate' &&
    (G['__NB_CREATED_CONFIGS'] as unknown[]).length === 1 &&
    G['__NB_OLD_DESTROYED'] === 1 &&
    Number(G['__NB_CONNECTIONS'] ?? 0) === connectionsBefore2 + 1,
    marker2.join('\n'));
  check('G4 its anchor is gen=2 and it becomes the accepted generation',
    lastLine('VPN_RECREATE_RESOLVED|') ===
    'VPN_RECREATE_RESOLVED|requestId=' + REQUEST_ID + '|gen=2|accepted=true' &&
    ability['n2hAcceptedExclusionGeneration'] === 2,
    lastLine('VPN_RECREATE_RESOLVED|'));
  check('G4 all three generations form the contiguous sequence 0,1,2 with kinds',
    String(gen0) + ',' + fieldOf(marker1[0], 'gen') + ',' + fieldOf(marker2[0], 'gen') ===
    '0,1,2' &&
    [fieldOf(marker0[0], 'kind'), fieldOf(marker1[0], 'kind'),
      fieldOf(marker2[0], 'kind')].join(',') === 'initial,recreate,recreate',
    [marker0[0], marker1[0], marker2[0]].join('\n'));

  // ---------------- R group (roam-1005-0124): pre-destroy TUN release, -----
  // ---------------- per-event resolver exclusion re-derivation -------------
  // A fresh ability with the frozen sets staged as the start flow would have
  // retained them on a HOME Wi-Fi boot: one management /32 (session-stable)
  // plus the home resolver 192.168.50.1/32. The network world is now the
  // PHYSICAL CELLULAR net the recreate runs on (its own resolver 10.64.64.64).
  {
    const cellAbility = freshAbility(mod, FIXTURE_FILESDIR);
    const frozenMgmt = { destination: '203.0.113.7', prefixLength: 32, gateway: '10.99.0.1',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: true };
    const frozenHomeDns = { destination: '192.168.50.1', prefixLength: 32, gateway: '10.99.0.1',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: true };
    cellAbility['frozenEndpointExclusions'] = [frozenMgmt, frozenHomeDns];
    cellAbility['frozenDnsEndpointExclusions'] = [frozenHomeDns];
    seedNetCtx({
      __NB_NET_DEFAULT_ID: 100,
      __NB_NET_CAPS_BY_ID: { 100: { networkCap: [15], bearerTypes: [0] } },
      __NB_NET_PROPS_BY_ID: {
        100: { dnses: [{ address: '10.64.64.64', family: 1, port: 53 }] }
      }
    });
    G['__NB_TUN_RELEASE_CALLS'] = 0;
    resetLog();
    await driveRecreate(cellAbility);
    // the release happens BEFORE the destroy→settle leg (its line precedes
    // the settle line) and the seam fired exactly once for the attempt
    const releaseLine = lastLine('VPN_RECREATE_TUN_RELEASE|');
    const logTextR = logs().join('\n');
    check('R1 the recreate releases the core TUN dup before the settle leg',
      releaseLine.indexOf('|result=ok|') > 0 &&
      releaseLine.indexOf('|released=true|') > 0 &&
      logTextR.indexOf('VPN_RECREATE_TUN_RELEASE|') <
      logTextR.indexOf('VPN_RECREATE_SETTLE|'),
      releaseLine);
    check('R1 the release seam fired exactly once for the accepted attempt',
      Number(G['__NB_TUN_RELEASE_CALLS'] ?? 0) === 1, String(G['__NB_TUN_RELEASE_CALLS']));
    // resolver re-derivation: the CURRENT network's resolvers replace the
    // crossed-network home /32; the frozen management /32 stays
    const dnsLine = lastLine('VPN_RECREATE_DNS_EXCLUSIONS|');
    check('R2 the recreate re-derives resolver exclusions from the CURRENT network',
      fieldOf(dnsLine, 'dns-branch') === 'default' &&
      fieldOf(dnsLine, 'resolvers') === '1' && fieldOf(dnsLine, 'routes') === '1' &&
      fieldOf(dnsLine, 'dnsGap') === 'none',
      dnsLine);
    const destsR = linesWith('VPN_ROUTE_ENTRY|').map(
      (l: string): string => fieldOf(l, 'dest'));
    check('R2 the fresh cellular resolver /32 is excluded',
      destsR.indexOf('10.64.64.64') >= 0, JSON.stringify(destsR));
    check('R2 the crossed-network home resolver /32 is NOT carried over',
      destsR.indexOf('192.168.50.1') < 0, JSON.stringify(destsR));
    check('R2 the frozen management /32 stays excluded',
      destsR.indexOf('203.0.113.7') >= 0, JSON.stringify(destsR));

    // a dns GAP (no physical net at all) drops the stale resolver set
    // honestly — never silently keeps the crossed-network freeze
    const gapAbility = freshAbility(mod, FIXTURE_FILESDIR);
    gapAbility['frozenEndpointExclusions'] = [frozenHomeDns];
    gapAbility['frozenDnsEndpointExclusions'] = [frozenHomeDns];
    seedNetCtx({});
    G['__NB_TUN_RELEASE_CALLS'] = 0;
    resetLog();
    await driveRecreate(gapAbility);
    const dnsLineGap = lastLine('VPN_RECREATE_DNS_EXCLUSIONS|');
    const destsGap = linesWith('VPN_ROUTE_ENTRY|').map(
      (l: string): string => fieldOf(l, 'dest'));
    check('R3 a dns gap yields NO resolver exclusion (never the stale set)',
      fieldOf(dnsLineGap, 'resolvers') === '0' &&
      fieldOf(dnsLineGap, 'routes') === '0' &&
      destsGap.indexOf('192.168.50.1') < 0,
      dnsLineGap + ' ' + JSON.stringify(destsGap));
    seedNetCtx({});
  }

  // ---------------- G5: three-way R-2 consistency via the REAL arm hook ----
  setArmFile(true, undefined);
  resetLog();
  (ability['maybeArmN2hCollector'] as (s: unknown) => void)(
    { wg: { device_up: true, ready: true } });
  const armed = lastLine('N2H_COLLECTOR_ARMED|');
  const armOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  const armedGen = fieldOf(armed, 'gen');
  check('G5 the arm carries the accepted generation as collector.exclusion_generation',
    armOpts['exclusion_generation'] === 2, JSON.stringify(armOpts));
  check('G5 the ARMED marker carries the same gen= field (R-2②)',
    armedGen === '2' && armed.indexOf('|gen=2|') > 0, armed);
  check('G5 the LAST ACCEPTED generation line carries the same gen (R-2③ three-way)',
    armedGen === String(armOpts['exclusion_generation']) &&
    armedGen === fieldOf(marker2[0], 'gen') &&
    armedGen === String(ability['n2hAcceptedExclusionGeneration']),
    JSON.stringify({ armed: armedGen, armOpts: armOpts['exclusion_generation'],
      marker: fieldOf(marker2[0], 'gen') }));
  check('G5 the ARMED marker keeps its existing fields (gen= is an insertion)',
    armed.indexOf('|openedMono=4242|exclusions=') > 0 &&
    armed.indexOf('|posctlPort=40001|probeWaitMs=800|peerB2aProbeId=none|') > 0 &&
    armed.endsWith('|diag=true|is_evidence=false'), armed);

  // ---------------- G6: a NOT-accepted create() reports no generation ------
  const rejectAbility = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  const genR = (rejectAbility['logExclusionGeneration'] as
    (kind: string, routes: unknown[], k: number) => number)(
    'initial', initialCfg['routes'] as unknown[], 0);
  G['__NB_FD_STATUS'] = { open: false, errno: 9 };
  const rejected = await (rejectAbility['settleCreate'] as
    (fd: number, gen: number) => Promise<Record<string, unknown>>)(7, genR);
  check('G6 a create() that is not accepted is NOT the accepted generation',
    rejected['accepted'] === false &&
    lastLine('VPN_CREATE_RESOLVED|').indexOf('|accepted=false|marker=CREATE_INVALID') > 0 &&
    rejectAbility['n2hAcceptedExclusionGeneration'] === -1,
    lastLine('VPN_CREATE_RESOLVED|') + ' ' + String(
      rejectAbility['n2hAcceptedExclusionGeneration']));
  setArmFile(true, undefined);
  resetLog();
  (rejectAbility['maybeArmN2hCollector'] as (s: unknown) => void)(
    { wg: { device_up: true, ready: true } });
  const armedR = lastLine('N2H_COLLECTOR_ARMED|');
  const optsR = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('G6 with no accepted generation the arm omits the key (never fabricates one)',
    optsR['exclusion_generation'] === undefined && fieldOf(armedR, 'gen') === '-1',
    JSON.stringify(optsR) + ' ' + armedR);

  // ---------------- A (I5a §13.15): the arm check is never a silent branch --
  // ① the arm file probe THROWS the documented ENOENT code: the hook must
  //    leave ONE grep-able skip line naming the reason and the raw code, and
  //    must NOT call the core arm. The ticket body seeded in the fs map proves
  //    nothing about the file rides the marker (the probe threw before any
  //    read). The path on the line is the RESOLVED filesDir join — the same
  //    basename the fixture EntryAbility writes (C group).
  const absentAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(true, 13900002, 'SENTINEL-ARM-BODY');
  resetLog();
  tickArm(absentAbility, WG_READY);
  const skipLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A1 a missing arm file emits exactly ONE skip marker',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1, JSON.stringify(logs()));
  check('A1 skip marker literal (reason=arm-file-absent + the raw access code + the resolved ' +
    'filesDir path)',
    skipLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-absent|path=' + ARM_TICKET_PATH + '|accessError=13900002' +
    '|attempted=true|diag=true|is_evidence=false', skipLine);
  check('A1 the skip does NOT call the core arm (no ARMED / NOT_ARMED / no opts)',
    G['__NB_ARM_OPTS'] === undefined && linesWith('N2H_COLLECTOR_ARMED|').length === 0 &&
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 0, JSON.stringify(logs()));
  check('A1 the attempt latch is visible on the line AND on the single latch field',
    fieldOf(skipLine, 'attempted') === 'true' && absentAbility['n2hArmAttempted'] === true,
    String(absentAbility['n2hArmAttempted']));
  check('A1 the skip line carries no arm-file body (nothing was read)',
    logs().join('\n').indexOf('SENTINEL-ARM-BODY') < 0, JSON.stringify(logs()));

  // ④ the one-shot latch: the ATTEMPT happens once per instance — the skip is
  //    printed on the first tick and never repeated, including a file that
  //    appears later on that same instance.
  resetLog();
  tickArm(absentAbility, WG_READY);
  check('A2 a second tick on the same instance repeats nothing (one-shot latch)',
    logs().length === 0, JSON.stringify(logs()));
  setArmFile(true, undefined);
  resetLog();
  tickArm(absentAbility, WG_READY);
  check('A2 a file appearing later never arms that instance (ONE latch, no second counter)',
    logs().length === 0 && G['__NB_ARM_OPTS'] === undefined &&
    absentAbility['n2hArmAttempted'] === true, JSON.stringify(logs()));

  // The API can also ANSWER "not found" as a boolean — the shape the 2026-09-18
  // device run actually produced (accessError=none; NOT a proven ENOENT). The
  // reason stays the same and the difference is visible in accessError.
  const falseAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, undefined);
  resetLog();
  tickArm(falseAbility, WG_READY);
  const falseLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A3 the boolean not-found answer is the SAME reason with accessError=none',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    falseLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-absent|path=' + ARM_TICKET_PATH + '|accessError=none' +
    '|attempted=true|diag=true|is_evidence=false', falseLine);

  // An error code that does NOT prove absence must be a DIFFERENT reason — the
  // two cases may never collapse into one (13900012 = permission denied).
  const unreadableAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, 13900012);
  resetLog();
  tickArm(unreadableAbility, WG_READY);
  const unreadableLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A4 a non-ENOENT error is reason=arm-file-unreadable with its code kept',
    unreadableLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-unreadable|path=' + ARM_TICKET_PATH + '|accessError=13900012' +
    '|attempted=true|diag=true|is_evidence=false', unreadableLine);
  const noCodeAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, 'no-code');
  resetLog();
  tickArm(noCodeAbility, WG_READY);
  const noCodeLine = lastLine('N2H_COLLECTOR_ARM_SKIPPED|');
  check('A4 an error thrown WITHOUT a code never claims absence (accessError=no-code)',
    noCodeLine === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' + REQUEST_ID +
    '|reason=arm-file-unreadable|path=' + ARM_TICKET_PATH + '|accessError=no-code' +
    '|attempted=true|diag=true|is_evidence=false', noCodeLine);

  // ② ③ (2026-09-18 real-device timing fix): with the ticket present but the
  // session gate closed the hook WAITS — one deduped WAITING line, no native
  // call, latch open — and the SAME instance arms exactly once on the first
  // later tick whose gate opens, ticket payload unchanged. NOT_ARMED is
  // retired: it consumed the only attempt while WG was still ramping (0016).
  const waitAbility = freshAbility(mod, FIXTURE_FILESDIR);
  const WAIT_TICKET = '{"posctl_port":41000,"probe_wait_ms":900}';
  setArmFile(true, undefined, WAIT_TICKET);
  resetLog();
  const callsAtWait = Number(G['__NB_ARM_CALLS'] ?? 0);
  tickArm(waitAbility, { wg: { device_up: false, ready: true } });
  check('A5 file present + gate closed emits ONE deduped WAITING line, no native call, ' +
    'latch open (NOT_ARMED retired)',
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    lastLine('N2H_COLLECTOR_WAITING|') ===
    'N2H_COLLECTOR_WAITING|requestId=' + REQUEST_ID +
    '|reason=session-not-ready|deviceUp=false|ready=true|attempted=false|' +
    'diag=true|is_evidence=false' &&
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 0 &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0 &&
    Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait &&
    waitAbility['n2hArmAttempted'] === false, JSON.stringify(logs()));
  tickArm(waitAbility, { wg: { device_up: true, ready: false } });
  check('A5 a second waiting tick (the 0016 shape deviceUp=true|ready=false) repeats NOTHING ' +
    '(dedup boolean: still the single first-tick line, zero native calls, latch open)',
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    linesWith('N2H_COLLECTOR_WAITING|').every((l: string): boolean =>
      l.indexOf('|deviceUp=false|ready=true|attempted=false|') > 0) &&
    linesWith('N2H_COLLECTOR_WAITING|').every((l: string): boolean =>
      l.indexOf('|deviceUp=true|') < 0) &&
    Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait &&
    waitAbility['n2hArmAttempted'] === false, JSON.stringify(logs()));
  waitAbility['n2hAcceptedExclusionGeneration'] = 0;
  tickArm(waitAbility, WG_READY);
  const waitOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('A5 the first ready tick arms ONCE on the SAME instance, ticket payload unchanged, ' +
    'no extra WAITING line',
    Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait + 1 &&
    waitOpts['posctl_port'] === 41000 && waitOpts['probe_wait_ms'] === 900 &&
    waitAbility['n2hArmAttempted'] === true &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 1 &&
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARMED|') ===
    'N2H_COLLECTOR_ARMED|requestId=' + REQUEST_ID +
    '|gen=0|openedMono=4242|exclusions=0|posctlPort=41000|probeWaitMs=900|' +
    'peerB2aProbeId=none|diag=true|is_evidence=false', JSON.stringify(logs()));
  resetLog();
  tickArm(waitAbility, WG_READY);
  check('A5 a tick after the arm repeats nothing (one-shot latch, no second native call)',
    logs().length === 0 && Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait + 1,
    JSON.stringify(logs()));
  const destroyedAbility = freshAbility(mod, FIXTURE_FILESDIR);
  destroyedAbility['destroyRequested'] = true;
  setArmFile(true, undefined); // present-but-empty ticket: A6 below reads the defaults
  resetLog();
  tickArm(destroyedAbility, WG_READY);
  check('A5 destroyRequested still arms nothing (unchanged guard: no line, no native call, ' +
    'latch stays open)',
    logs().length === 0 && Number(G['__NB_ARM_CALLS'] ?? 0) === callsAtWait + 1 &&
    destroyedAbility['n2hArmAttempted'] === false, JSON.stringify(logs()));
  const readyAbility = freshAbility(mod, FIXTURE_FILESDIR);
  readyAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(readyAbility, WG_READY);
  check('A6 file present + gate open still ARMs, existing literal unchanged (regression)',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0 &&
    lastLine('N2H_COLLECTOR_ARMED|') ===
    'N2H_COLLECTOR_ARMED|requestId=' + REQUEST_ID +
    '|gen=0|openedMono=4242|exclusions=0|posctlPort=40001|probeWaitMs=800|' +
    'peerB2aProbeId=none|diag=true|is_evidence=false', JSON.stringify(logs()));

  // ---------------- G7: wiring pins on the byte-identical source -----------
  const extSource: string = readFileSync(join(dir, 'NetBirdVpnExtensionAbility.ts')).toString();
  const initialCall = extSource.indexOf(
    'this.logExclusionGeneration(EXCLUSION_GENERATION_INITIAL, merged.routes, 0);');
  const initialDump = extSource.indexOf('this.logRouteTable(merged, gen);');
  const initialCreate = extSource.indexOf('const rawCreate: Promise<number> = connection.create(config);');
  const initialAnchor = extSource.indexOf('return this.settleCreate(fd, gen);');
  check('G7 initial leg: marker -> rows -> create() -> anchor, in that order',
    initialCall > 0 && initialCall < initialDump && initialCall < initialCreate &&
    initialCreate < initialAnchor, JSON.stringify({ initialCall, initialDump,
      initialCreate, initialAnchor }));
  const recreateCall = extSource.indexOf(
    'this.logExclusionGeneration(EXCLUSION_GENERATION_RECREATE, merged.routes, 0);');
  const recreateDump = extSource.indexOf('this.logRouteTable(merged, gen);', initialDump + 1);
  const recreateCreate = extSource.indexOf(
    'const fd: number = await this.createConnectionBounded(conn, config);');
  const recreateAnchorAt = extSource.indexOf(
    "'VPN_RECREATE_RESOLVED|requestId=%{public}s|gen=%{public}d|accepted=true'");
  check('G7 recreate leg: marker -> rows -> create() -> VPN_RECREATE_RESOLVED, in that order',
    recreateCall > 0 && recreateCall < recreateDump && recreateCall < recreateCreate &&
    recreateCreate < recreateAnchorAt, JSON.stringify({ recreateCall, recreateDump,
      recreateCreate, recreateAnchorAt }));
  check('G7 the initial acceptance anchor literal is UNCHANGED (no gen= added)',
    extSource.indexOf(
      "'VPN_CREATE_RESOLVED|requestId=%{public}s|fd=%{public}s|accepted=%{public}s|marker=%{public}s'")
    > 0, 'anchor pin');
  check('G7 the pre-existing recreate marker literals are UNCHANGED',
    extSource.indexOf(
      "'VPN_RECREATE_SNAPSHOT|requestId=%{public}s|serial=%{public}s|routes=%{public}d|'") > 0 &&
    extSource.indexOf(
      "'VPN_RECREATE_DONE|requestId=%{public}s|tunFd=%{public}d|routes=%{public}d|'") > 0,
    'recreate line pin');
  check('G7 the arm reads its generation from ONE field (R-2 same-source)',
    extSource.indexOf(
      'const exclusionGeneration: number = this.n2hAcceptedExclusionGeneration;') > 0 &&
    extSource.indexOf('opts.exclusion_generation = exclusionGeneration;') > 0 &&
    extSource.indexOf(
      "'N2H_COLLECTOR_ARMED|requestId=%{public}s|gen=%{public}d|openedMono=%{public}d|'") > 0,
    'arm wiring pin');
  check('G7 the wgPeerEndpoints field is passed at BOTH legs (value 0 until I3b)',
    extSource.indexOf('merged.routes, 0);') > 0 &&
    extSource.split('merged.routes, 0);').length === 3, 'wg_peer field pin');

  // ---------------- B (I5b §13.16): the two PATH beacons + 三态矩阵 ---------
  // The watcher's interval is CAPTURED, never scheduled: the B group replays
  // REAL ticks by hand, which is precisely the "the watcher was started but
  // never ticked" form the device dry-run could not distinguish from "the hook
  // ran and skipped". No real timer is left pending (the G legs never reach the
  // watcher: their protect gate fails closed first on the absent wg_fwd_open).
  const capturedTicks: Array<() => void> = [];
  G['setInterval'] = (handler: () => void): number => {
    capturedTicks.push(handler);
    return 7001;
  };
  // The live-session document the REAL tick polls: wg up + ready, not terminal,
  // no recreate ⇒ the tick reaches the arm hook on every replay.
  const watcherStatus: Record<string, unknown> = {
    running: true, state: 'connected', peer_count: 1, route_count: 2,
    reconnects: 0, terminal: false, wg: { device_up: true, ready: true }
  };
  const pathLines = (stage: string): string[] => linesWith('N2H_COLLECTOR_PATH|')
    .filter((l: string): boolean => fieldOf(l, 'stage') === stage);

  // B1: the onCreate beacon through the REAL onCreate (normal path; the async
  // start flow aborts on the fixture's missing credentials and never reaches the
  // watcher). The context is present — the production shape, and required by
  // onCreate's diagnostics tee (2026-10-08: `startDiagnosticsLogTee` reads
  // `this.context.filesDir` synchronously), which is why this instance is no
  // longer created context-free; no timer or watcher beacon is produced either
  // way, so B2/B3 below observe exactly the previous shape.
  const bornAbility = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  (bornAbility['onCreate'] as (w: unknown) => void)(
    { bundleName: 'com.n2h.fixture', parameters: { requestId: REQUEST_ID } });
  const onCreateBeacons = pathLines('onCreate-reached');
  const onCreateLine: string = onCreateBeacons.length > 0 ? onCreateBeacons[0] : '';
  check('B1 onCreate emits exactly ONE stage=onCreate-reached beacon',
    onCreateBeacons.length === 1, JSON.stringify(linesWith('N2H_COLLECTOR_PATH|')));
  check('B1 onCreate beacon literal (5 fields: requestId|stage|diag|is_evidence)',
    onCreateLine === 'N2H_COLLECTOR_PATH|requestId=' + REQUEST_ID +
    '|stage=onCreate-reached|diag=true|is_evidence=false' &&
    onCreateLine.split('|').length === 5, onCreateLine);
  check('B1 the beacon carries the requestId resolved from the want (never the default)',
    fieldOf(onCreateLine, 'requestId') === REQUEST_ID &&
    fieldOf(onCreateLine, 'requestId') !== 'missing', onCreateLine);
  await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 0); });
  check('B1 the async start flow of that onCreate never re-prints the beacon',
    pathLines('onCreate-reached').length === 1, JSON.stringify(logs()));

  // B2: the beacon precedes EVERY early return — the earliest reachable point.
  const dupAbility = freshAbility(mod);
  dupAbility['connectionCreationAttempted'] = true;
  resetLog();
  (dupAbility['onCreate'] as (w: unknown) => void)(
    { bundleName: 'com.n2h.fixture', parameters: { requestId: REQUEST_ID } });
  check('B2 the beacon is at the entry: it precedes the duplicate-onCreate return',
    linesWith('N2H_COLLECTOR_PATH|').length === 1 &&
    lastLine('VPN_CREATE_REJECTED|') === 'VPN_CREATE_REJECTED|requestId=' + REQUEST_ID +
    '|phase=duplicate-onCreate|summary=connection-already-attempted', JSON.stringify(logs()));

  // B3: the watcher beacon — one line per instance, never per tick, through the
  // REAL start method + REPLAYED REAL ticks (matrix ①: beacons + absent file).
  const tickAbility = freshAbility(mod, FIXTURE_FILESDIR);
  G['__NB_STATUS'] = watcherStatus;
  setArmFile(false, undefined); // the device form: boolean false (accessError=none)
  resetLog();
  (tickAbility['startStatusAndNetcfgWatcher'] as () => void)();
  const watcherBeaconLine: string = pathLines('watcher-started').length > 0
    ? pathLines('watcher-started')[0] : '';
  check('B3 the watcher start emits exactly ONE stage=watcher-started beacon',
    pathLines('watcher-started').length === 1 && capturedTicks.length === 1,
    JSON.stringify({ beacons: pathLines('watcher-started').length,
      ticks: capturedTicks.length }));
  check('B3 watcher beacon literal (5 fields: requestId|stage|diag|is_evidence)',
    watcherBeaconLine === 'N2H_COLLECTOR_PATH|requestId=' + REQUEST_ID +
    '|stage=watcher-started|diag=true|is_evidence=false' &&
    watcherBeaconLine.split('|').length === 5, watcherBeaconLine);
  (tickAbility['startStatusAndNetcfgWatcher'] as () => void)();
  check('B3 a second start call re-prints nothing (the existing once-guard, no new latch)',
    pathLines('watcher-started').length === 1 && capturedTicks.length === 1,
    JSON.stringify({ beacons: pathLines('watcher-started').length,
      ticks: capturedTicks.length }));
  for (let i = 0; i < 3; i++) {
    capturedTicks[0]();
  }
  check('B3 three REPLAYED real ticks keep ONE watcher beacon (never per-tick spam)',
    pathLines('watcher-started').length === 1, JSON.stringify(linesWith('N2H_COLLECTOR_PATH|')));
  check('B3 matrix ①: beacons + arm file absent ⇒ exactly ONE ARM_SKIPPED (I5a regression)',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|') === 'N2H_COLLECTOR_ARM_SKIPPED|requestId=' +
    REQUEST_ID + '|reason=arm-file-absent|path=' + ARM_TICKET_PATH + '|' +
    'accessError=none|attempted=true|diag=true|is_evidence=false',
    JSON.stringify(linesWith('N2H_COLLECTOR_ARM_SKIPPED|')));

  // B4: matrix ② — the extension AND the watcher were reached, but the watcher
  // never ticked: both beacons, and NOT ONE arm line.
  const idleAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, undefined);
  resetLog();
  (idleAbility['onCreate'] as (w: unknown) => void)(
    { bundleName: 'com.n2h.fixture', parameters: { requestId: REQUEST_ID } });
  (idleAbility['startStatusAndNetcfgWatcher'] as () => void)();
  check('B4 matrix ②: beacons present + ZERO ticks ⇒ both beacons and NO arm line',
    JSON.stringify(linesWith('N2H_COLLECTOR_PATH|')
      .map((l: string): string => fieldOf(l, 'stage'))) ===
    JSON.stringify(['onCreate-reached', 'watcher-started']) &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0 &&
    linesWith('N2H_COLLECTOR_NOT_ARMED|').length === 0 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 0, JSON.stringify(logs()));

  // B5: matrix ③ — the extension process never started: no beacon at all.
  const silentAbility = freshAbility(mod);
  resetLog();
  check('B5 matrix ③: an instance that received no lifecycle call emits NO beacon',
    silentAbility['requestId'] === REQUEST_ID && linesWith('N2H_COLLECTOR_PATH|').length === 0 &&
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0, JSON.stringify(logs()));

  // B6: source pins on the byte-identical fixture (positions + single copies).
  const onCreatePathAt = extSource.indexOf(
    "'N2H_COLLECTOR_PATH|requestId=%{public}s|stage=onCreate-reached|diag=true|is_evidence=false'");
  const duplicateRejectAt = extSource.indexOf(
    "'VPN_CREATE_REJECTED|requestId=%{public}s|phase=duplicate-onCreate|summary=connection-already-attempted'");
  check('B6 the onCreate beacon is pinned BEFORE the duplicate-onCreate early return',
    onCreatePathAt > 0 && onCreatePathAt < duplicateRejectAt,
    JSON.stringify({ onCreatePathAt, duplicateRejectAt }));
  const watcherPathAt = extSource.indexOf(
    "'N2H_COLLECTOR_PATH|requestId=%{public}s|stage=watcher-started|diag=true|is_evidence=false'");
  const watcherGuardAt = extSource.indexOf(
    'if (this.watcherTimer >= 0 || this.destroyRequested) {');
  const intervalAt = extSource.indexOf('}, STATUS_POLL_MS);');
  check('B6 the watcher beacon is pinned AFTER the once-guard and the interval creation',
    watcherPathAt > 0 && watcherPathAt > watcherGuardAt && watcherPathAt > intervalAt,
    JSON.stringify({ watcherPathAt, watcherGuardAt, intervalAt }));
  check('B6 each beacon literal exists exactly ONCE in the shipped source (no extra copy)',
    extSource.split('stage=onCreate-reached').length === 2 &&
    extSource.split('stage=watcher-started').length === 2, 'beacon literal pin');

  // ---------------- C (N2-H arm private channel, 2026-09-18): the REAL
  // EntryAbility want -> filesDir ticket -> REAL extension read -> native arm.
  // Both fixtures are the SHIPPED files; the path-aware fake fs keys its map
  // by the RESOLVED path, so the two processes meet on ONE file — exactly the
  // cross-process filesDir mechanism the device already validated for the
  // credentials.
  const entryMod = await import(pathToFileURL(join(dir, 'EntryAbility.ts')).href) as
    Record<string, unknown>;
  const ARM_PAYLOAD = '{"posctl_port":41000,"probe_wait_ms":900,' +
    '"peer_b2a_probe_id":"N2H-posctl-fixture"}';
  const newEntry = (): Record<string, unknown> => {
    const entry = new (entryMod['default'] as new () => Record<string, unknown>)();
    entry['context'] = { filesDir: FIXTURE_FILESDIR };
    return entry;
  };
  const resetFs = (): void => {
    G['__NB_FS'] = {};
    G['__NB_FS_ACCESS_ERROR'] = {};
    G['__NB_FS_OPEN_ERROR'] = {};
    G['__NB_FS_WRITE_ERROR'] = {};
    G['__NB_ARM_OPTS'] = undefined;
  };

  // C1: the explicit non-secret want lands byte-verbatim in filesDir, logged
  // as file name + byte count, payload never printed. The count is the payload's
  // real UTF-8 byte length (never the UTF-16 code-unit string length).
  resetFs();
  const writeEntry = newEntry();
  resetLog();
  (writeEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C1 the want payload is written VERBATIM to <filesDir>/netbird-n2h-arm',
    fileOf(ARM_TICKET_PATH) === ARM_PAYLOAD, JSON.stringify(fileOf(ARM_TICKET_PATH)));
  check('C1 the write is logged as file name + UTF-8 byte count only',
    linesWith('N2H_ARM_TICKET_WRITTEN|').length === 1 &&
    lastLine('N2H_ARM_TICKET_WRITTEN|') === 'N2H_ARM_TICKET_WRITTEN|file=' + ARM_TICKET_NAME +
    '|bytes=' + String(Buffer.byteLength(ARM_PAYLOAD, 'utf8')) +
    '|note=diagnostic-only-non-secret',
    linesWith('N2H_ARM_TICKET_WRITTEN|').join('\n'));
  check('C1 the payload never reaches the log',
    logs().join('\n').indexOf('N2H-posctl-fixture') < 0, JSON.stringify(logs()));

  // C1b: the logged byte count is UTF-8 bytes — a multibyte payload exposes any
  // UTF-16 code-unit impersonation (2 chars cost 6 UTF-8 bytes, not 2).
  const UNI_PAYLOAD = '{"peer_b2a_probe_id":"N2H-λ-超"}';
  resetLog();
  (writeEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': UNI_PAYLOAD } }, {});
  check('C1b the byte count is UTF-8 (multibyte payload), file byte-verbatim',
    fileOf(ARM_TICKET_PATH) === UNI_PAYLOAD &&
    lastLine('N2H_ARM_TICKET_WRITTEN|').indexOf('|bytes=' +
      String(Buffer.byteLength(UNI_PAYLOAD, 'utf8')) + '|') > 0 &&
    Buffer.byteLength(UNI_PAYLOAD, 'utf8') !== UNI_PAYLOAD.length,
    lastLine('N2H_ARM_TICKET_WRITTEN|') + ' vs len=' + UNI_PAYLOAD.length);
  seedFile(ARM_TICKET_PATH, ARM_PAYLOAD); // C2 reads the C1 payload, not C1b's

  // C2: the REAL extension probes + reads the SAME basename under the SAME
  // filesDir and forwards the EXISTING arm options to the native arm.
  const channelAbility = freshAbility(mod, FIXTURE_FILESDIR);
  channelAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(channelAbility, WG_READY);
  const channelOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C2 the extension reads the filesDir ticket and sends the existing arm options to native',
    channelOpts['posctl_port'] === 41000 && channelOpts['probe_wait_ms'] === 900 &&
    channelOpts['peer_b2a_probe_id'] === 'N2H-posctl-fixture',
    JSON.stringify(channelOpts));
  check('C2 the ARMED marker carries the announced peer and the non-default knobs',
    lastLine('N2H_COLLECTOR_ARMED|') === 'N2H_COLLECTOR_ARMED|requestId=' + REQUEST_ID +
    '|gen=0|openedMono=4242|exclusions=0|posctlPort=41000|probeWaitMs=900|' +
    'peerB2aProbeId=N2H-posctl-fixture|diag=true|is_evidence=false',
    lastLine('N2H_COLLECTOR_ARMED|'));
  check('C2 a present ticket produces NO skip line',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 0, JSON.stringify(logs()));

  // C3: a launch without diagnostic parameters deletes both stale diagnostic
  // files; credentials and device config remain byte-identical.
  const SETUP_PATH = `${FIXTURE_FILESDIR}/netbird-setup-key`;
  const DEVCFG_PATH = `${FIXTURE_FILESDIR}/netbird-device-config.json`;
  const DEVIATION_PATH = `${FIXTURE_FILESDIR}/netbird-protect-deviation`;
  seedFile(ARM_TICKET_PATH, '{"posctl_port":41000}'); // leftover diagnostic round
  seedFile(SETUP_PATH, 'setup-key-body-untouched');
  seedFile(DEVCFG_PATH, '{"management_url":"http://m","private_key":"pk-body"}');
  seedFile(DEVIATION_PATH, 'route-exclusion');
  const normalEntry = newEntry();
  resetLog();
  (normalEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird' }, {}); // NO parameters at all
  check('C3 a parameter-less launch deletes the leftover ticket (default: no arm)',
    fileOf(ARM_TICKET_PATH) === undefined &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 1 &&
    lastLine('N2H_ARM_TICKET_REMOVED|') === 'N2H_ARM_TICKET_REMOVED|file=' + ARM_TICKET_NAME +
    '|note=default-no-arm', JSON.stringify(logs()));
  check('C3 a parameter-less launch clears stale deviation, preserving credentials/config',
    fileOf(SETUP_PATH) === 'setup-key-body-untouched' &&
    fileOf(DEVCFG_PATH) === '{"management_url":"http://m","private_key":"pk-body"}' &&
    fileOf(DEVIATION_PATH) === undefined &&
    linesWith('PROTECT_DEVIATION_REMOVED|').length === 1, JSON.stringify(logs()));
  const cleanAbility = freshAbility(mod, FIXTURE_FILESDIR);
  G['__NB_ARM_OPTS'] = undefined;
  resetLog();
  tickArm(cleanAbility, WG_READY);
  check('C3 after the cleanup the extension reports arm-file-absent and never arms',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|').indexOf('|reason=arm-file-absent|') > 0 &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));
  // A credentials-bearing launch WITHOUT the arm param behaves the same. (Its
  // credential writes follow the PRE-EXISTING flow verbatim — including
  // netbird_config overwriting netbird-device-config.json.)
  const credEntry = newEntry();
  resetLog();
  (credEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'netbird_setup_key': 'K2', 'netbird_config': '{"a":1}' } }, {});
  check('C3 a credential launch without the arm param also clears the ticket, credentials intact',
    fileOf(ARM_TICKET_PATH) === undefined && fileOf(SETUP_PATH) === 'K2\n' &&
    fileOf(DEVCFG_PATH) === '{"a":1}' && fileOf(DEVIATION_PATH) === undefined,
    JSON.stringify({ setup: fileOf(SETUP_PATH), devcfg: fileOf(DEVCFG_PATH),
      arm: fileOf(ARM_TICKET_PATH) }));
  const deviationEntry = newEntry();
  resetLog();
  (deviationEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { protectDeviation: 'route-exclusion' } }, {});
  check('C3 explicit route-exclusion launch writes the opt-in deviation file',
    fileOf(DEVIATION_PATH) === 'route-exclusion\n' &&
    fileOf(ARM_TICKET_PATH) === undefined &&
    fileOf(SETUP_PATH) === 'K2\n' && fileOf(DEVCFG_PATH) === '{"a":1}',
    JSON.stringify({ deviation: fileOf(DEVIATION_PATH), arm: fileOf(ARM_TICKET_PATH) }));

  // C4: an invalid-JSON payload keeps the ORIGINAL convention — the file is
  // written verbatim, the extension logs ARM_OPTS_IGNORED and falls back to
  // the §5.1 defaults; the ARMED marker shows the defaults (a REAL arming
  // under defaults, never the refused options smuggled through as applied).
  resetFs();
  const badEntry = newEntry();
  (badEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': '{not-json' } }, {});
  check('C4 an invalid-JSON payload is written verbatim (the extension owns the parse)',
    fileOf(ARM_TICKET_PATH) === '{not-json', JSON.stringify(fileOf(ARM_TICKET_PATH)));
  const badAbility = freshAbility(mod, FIXTURE_FILESDIR);
  badAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(badAbility, WG_READY);
  check('C4 invalid JSON keeps the original convention: ARM_OPTS_IGNORED + defaults, still armed',
    linesWith('N2H_COLLECTOR_ARM_OPTS_IGNORED|').length === 1 &&
    fieldOf(lastLine('N2H_COLLECTOR_ARM_OPTS_IGNORED|'), 'action') === 'defaults' &&
    lastLine('N2H_COLLECTOR_ARMED|').indexOf('|posctlPort=40001|probeWaitMs=800|') > 0 &&
    lastLine('N2H_COLLECTOR_ARMED|').indexOf('peerB2aProbeId=none|') > 0,
    JSON.stringify(logs()));

  // C5: a failed OPEN leaves NO ticket and is logged with its error CODE —
  // the extension then sees arm-file-absent; nobody fakes an arm.
  resetFs();
  G['__NB_FS_OPEN_ERROR'] = { [ARM_TICKET_PATH]: 13900042 };
  const failEntry = newEntry();
  resetLog();
  (failEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C5 a failed open leaves NO ticket and logs the error CODE (never the payload)',
    fileOf(ARM_TICKET_PATH) === undefined &&
    lastLine('N2H_ARM_TICKET_WRITE_FAILED|') === 'N2H_ARM_TICKET_WRITE_FAILED|file=' +
    ARM_TICKET_NAME + '|code=13900042|note=no-ticket-no-arm' &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 0,
    linesWith('N2H_ARM_TICKET_WRITE_FAILED|').join('\n') + '\n' +
    linesWith('N2H_ARM_TICKET_REMOVED|').join('\n'));
  const failAbility = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  tickArm(failAbility, WG_READY);
  check('C5 after a failed open the extension reports arm-file-absent (no faked arm)',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|').indexOf('|reason=arm-file-absent|') > 0 &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));

  // C5b: the dangerous shape — the OPEN succeeded (its TRUNC wiped the
  // previous round's ticket) and the WRITE then threw: the on-disk file is an
  // empty ticket the extension would parse into the §5.1 defaults and ARM.
  // The failure path must REMOVE exactly this one diagnostic ticket (logged
  // independently), never touch the credentials; the separate deviation
  // cleanup still runs because its opt-in parameter is absent.
  resetFs();
  seedFile(ARM_TICKET_PATH, '{"posctl_port":41000}'); // the previous ticket, now TRUNC-wiped
  seedFile(SETUP_PATH, 'setup-key-body-untouched');
  seedFile(DEVCFG_PATH, '{"management_url":"http://m","private_key":"pk-body"}');
  seedFile(DEVIATION_PATH, 'route-exclusion');
  G['__NB_FS_WRITE_ERROR'] = { [ARM_TICKET_PATH]: 13900001 };
  const halfEntry = newEntry();
  resetLog();
  (halfEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C5b a failed write after TRUNC removes the empty ticket (it must not survive)',
    fileOf(ARM_TICKET_PATH) === undefined, JSON.stringify(fileOf(ARM_TICKET_PATH)));
  check('C5b the write failure and the cleanup are logged independently with codes',
    lastLine('N2H_ARM_TICKET_WRITE_FAILED|') === 'N2H_ARM_TICKET_WRITE_FAILED|file=' +
    ARM_TICKET_NAME + '|code=13900001|note=no-ticket-no-arm' &&
    lastLine('N2H_ARM_TICKET_REMOVED|') === 'N2H_ARM_TICKET_REMOVED|file=' + ARM_TICKET_NAME +
    '|note=failed-write-cleanup', JSON.stringify(logs()));
  check('C5b credentials/config survive failed arm write; stale deviation is cleared',
    fileOf(SETUP_PATH) === 'setup-key-body-untouched' &&
    fileOf(DEVCFG_PATH) === '{"management_url":"http://m","private_key":"pk-body"}' &&
    fileOf(DEVIATION_PATH) === undefined &&
    linesWith('PROTECT_DEVIATION_REMOVED|').length === 1, JSON.stringify(logs()));
  const halfAbility = freshAbility(mod, FIXTURE_FILESDIR);
  G['__NB_ARM_OPTS'] = undefined;
  resetLog();
  tickArm(halfAbility, WG_READY);
  check('C5b the extension then SKIPS (arm-file-absent) and never calls the native arm',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    lastLine('N2H_COLLECTOR_ARM_SKIPPED|').indexOf('|reason=arm-file-absent|') > 0 &&
    G['__NB_ARM_OPTS'] === undefined, JSON.stringify(logs()));
  // C5c: if even the CLEANUP fails, the surviving (truncated) file is reported
  // as what it is — REMOVE_FAILED with its code — and never as removed.
  resetFs();
  seedFile(ARM_TICKET_PATH, '{"posctl_port":41000}');
  G['__NB_FS_WRITE_ERROR'] = { [ARM_TICKET_PATH]: 13900001 };
  G['__NB_FS_ACCESS_ERROR'] = { [ARM_TICKET_PATH]: 13900042 };
  const stuckEntry = newEntry();
  resetLog();
  (stuckEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('C5c a failed cleanup keeps the truncated file and logs REMOVE_FAILED (no false removal)',
    fileOf(ARM_TICKET_PATH) === '' &&
    lastLine('N2H_ARM_TICKET_REMOVE_FAILED|') === 'N2H_ARM_TICKET_REMOVE_FAILED|file=' +
    ARM_TICKET_NAME + '|code=13900042|note=failed-write-cleanup' &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 0, JSON.stringify(logs()));

  // C6: the per-instance ONE-SHOT arm attempt is intact on the new channel —
  // the second tick of the same instance never calls the native arm again.
  resetFs();
  setArmFile(true, undefined, ARM_PAYLOAD);
  const latchAbility = freshAbility(mod, FIXTURE_FILESDIR);
  latchAbility['n2hAcceptedExclusionGeneration'] = 0;
  resetLog();
  tickArm(latchAbility, WG_READY);
  const callsAfterFirst: number = Number(G['__NB_ARM_CALLS'] ?? 0);
  check('C6 the first tick with a ticket calls the native arm exactly once',
    callsAfterFirst >= 1 && linesWith('N2H_COLLECTOR_ARMED|').length === 1,
    JSON.stringify(logs()));
  resetLog();
  tickArm(latchAbility, WG_READY);
  tickArm(latchAbility, WG_READY);
  check('C6 the SECOND tick of the same instance repeats NOTHING (no second native call)',
    logs().length === 0 && Number(G['__NB_ARM_CALLS'] ?? 0) === callsAfterFirst,
    JSON.stringify({ logs: logs(), calls: G['__NB_ARM_CALLS'] }));

  // C7: source pins — the extension has NO app-invisible arm path left, the
  // channel rides the SAME filesDir mechanism as the credentials, and the
  // credential dev-override channel is untouched.
  const entrySource: string = readFileSync(join(dir, 'EntryAbility.ts')).toString();
  check('C7 the extension no longer references the app-invisible /data/local/tmp arm path',
    extSource.indexOf('/data/local/tmp/netbird-n2h-arm') < 0 &&
    extSource.indexOf("const N2H_ARM_FILE_NAME: string = 'netbird-n2h-arm';") > 0 &&
    extSource.indexOf('${this.context.filesDir}/${N2H_ARM_FILE_NAME}') > 0,
    'arm path pin');
  check('C7 the EntryAbility carries the non-secret param, the same basename, and the cleanup',
    entrySource.indexOf("'n2h_arm_options'") > 0 &&
    entrySource.indexOf("'netbird-n2h-arm'") > 0 &&
    entrySource.indexOf("this.removeArmTicket('default-no-arm')") > 0 &&
    entrySource.indexOf("this.removeArmTicket('failed-write-cleanup')") > 0 &&
    entrySource.indexOf('short-write:') > 0 &&
    entrySource.indexOf('new util.TextEncoder().encode(text).byteLength') > 0 &&
    entrySource.indexOf('N2H_ARM_TICKET_WRITTEN') > 0,
    'entry channel pin');
  check('C7 the credential dev-override channel is untouched',
    readFileSync(join(dir, 'NetBirdConnector.ts')).toString()
      .indexOf('DEV_CREDENTIAL_DIR') > 0,
    'credential override pin');

  // ---------------- N (N2 T3 §3, 2026-09-28): the positive-control private
  // ---------------- channel — the REAL EntryAbility write/cleanup/combo rule
  // ---------------- and the REAL armed-only poll + tick ordering.
  //
  // The channel is the SAME three-file mechanism as the arm ticket / stimulus:
  // EntryAbility writes <filesDir>/netbird-n2-control from the non-secret
  // `n2_control` want parameter, the REAL extension's `pollN2Control` consumes
  // it once and calls `sendN2PositiveControl` (the recording seam above), and
  // the core's `N2_CONTROL_SENT`/`_HIT`/`_MISS` lines stay authoritative.
  const CONTROL_PATH = `${FIXTURE_FILESDIR}/netbird-n2-control`;
  const CONTROL_NAME = 'netbird-n2-control';
  const STIMULUS_PATH = `${FIXTURE_FILESDIR}/netbird-n2-stimulus`;
  const CONTROL_PAYLOAD = '{"target":"119.29.29.119:53","proto":"udp","id":"evt1-pre"}';
  const STIMULUS_PAYLOAD = '{"kind":"M","id":"b-M-1"}';
  const N2_EVIDENCE_OPTIONS: Record<string, unknown> = {
    trustedSelf: true, capture32: ['119.29.29.119'],
    outerDestinations: ['udp/119.29.29.119:53', 'tcp/119.29.29.119:443'],
    hdcControlIp: '', allowHdcOverlap: false
  };
  const resetRequestState = (): void => {
    G['__NB_CONTROL_CALLS'] = undefined;
    G['__NB_CONTROL_LAST'] = undefined;
    G['__NB_CONTROL_IDS'] = undefined;
    G['__NB_CONTROL_MODE'] = undefined;
    G['__NB_CONTROL_ERROR'] = undefined;
    G['__NB_STIMULUS_CALLS'] = undefined;
    G['__NB_STIMULUS_ANSWER'] = undefined;
    G['__NB_A_EVENT_CALLS'] = undefined;
    G['__NB_A_EVENT_LAST'] = undefined;
    G['__NB_A_EVENT_ANSWER'] = undefined;
    G['__NB_SANDWICH_PRE_CALLS'] = undefined;
    G['__NB_SANDWICH_PRE_LAST'] = undefined;
    G['__NB_SANDWICH_PRE_ANSWER'] = undefined;
    G['__NB_SANDWICH_POST_CALLS'] = undefined;
    G['__NB_SANDWICH_POST_LAST'] = undefined;
    G['__NB_SANDWICH_POLL_CALLS'] = undefined;
    G['__NB_SANDWICH_POLL_ANSWER'] = undefined;
    // N2 first batch C/D columns (2026-09-29): the switching-sandwich and ring
    // seams of the SAME recording core stub.
    G['__NB_CD_PLAN_CALLS'] = undefined;
    G['__NB_CD_PLAN_LAST'] = undefined;
    G['__NB_CD_PLAN_ANSWER'] = undefined;
    G['__NB_CD_PRE_CALLS'] = undefined;
    G['__NB_CD_PRE_LAST'] = undefined;
    G['__NB_CD_PRE_ANSWER'] = undefined;
    G['__NB_CD_PREPARE_CALLS'] = undefined;
    G['__NB_CD_PREPARE_LAST'] = undefined;
    G['__NB_CD_PREPARE_ANSWER'] = undefined;
    G['__NB_CD_READY_CALLS'] = undefined;
    G['__NB_CD_READY_LAST'] = undefined;
    G['__NB_CD_READY_ANSWER'] = undefined;
    G['__NB_CD_IDENTITY_CALLS'] = undefined;
    G['__NB_CD_IDENTITY_LAST'] = undefined;
    G['__NB_CD_IDENTITY_ANSWER'] = undefined;
    G['__NB_CD_SWITCH_CALLS'] = undefined;
    G['__NB_CD_SWITCH_LAST'] = undefined;
    G['__NB_CD_SWITCH_ANSWER'] = undefined;
    G['__NB_CD_POST_CALLS'] = undefined;
    G['__NB_CD_POST_LAST'] = undefined;
    G['__NB_CD_POST_ANSWER'] = undefined;
    G['__NB_CD_POLL_CALLS'] = undefined;
    G['__NB_CD_POLL_ANSWER'] = undefined;
    G['__NB_CD_POLL_ANSWER_QUEUE'] = undefined;
    G['__NB_RING'] = undefined;
    G['__NB_RING_CONFIG'] = undefined;
    G['__NB_RING_DISABLED'] = undefined;
    G['__NB_RING_STATUS_CALLS'] = undefined;
    G['__NB_RING_CONFIGURE_ANSWER'] = undefined;
    G['__NB_RING_MANIFEST_CALLS'] = undefined;
    G['__NB_RING_MANIFEST_ANSWER'] = undefined;
    G['__NB_RING_DUMP_CALLS'] = undefined;
    G['__NB_RING_DUMP_LAST'] = undefined;
    G['__NB_RING_DUMP_ANSWER'] = undefined;
    G['__NB_RING_ACK_CALLS'] = undefined;
    G['__NB_RING_ACK_LAST'] = undefined;
    G['__NB_RING_ACK_ANSWER'] = undefined;
    // 2026-09-29 (explicit host ACK): the throw switch of the ACK seam.
    G['__NB_CORE_ACK_THROW'] = undefined;
    // 2026-09-30 (formal-switch production-recovery gate): the production-supply
    // counters/records the hook bumps at the core seams, the parked-DNS answer,
    // the simulated core clock and the minimal C/D core-plan mirror.
    resetSupply();
    G['__NB_DNS_POLL'] = undefined;
    G['__NB_CD_CORE'] = undefined;
    G['__NB_CD_CORE_NOW'] = undefined;
    G['__NB_SOCKET_SUPPLY_REQUEST'] = undefined;
    G['__NB_WG_FD'] = undefined;
    G['__NB_ICE_FD'] = undefined;
    G['__NB_ORDER'] = undefined;
  };
  // Every core seam that hands the core a production socket, one counter each
  // (exclusion-generation-hooks.mjs). `supplyTotal()` is the SINGLE number the
  // gate assertions read: "the gate let nothing through" = 0.
  const SUPPLY_KEYS: string[] = ['__NB_SUPPLY_DNS', '__NB_SUPPLY_DNS_RESOLVE',
    '__NB_SUPPLY_WG_OPEN', '__NB_SUPPLY_WG_FEED', '__NB_SUPPLY_MGMT_OPEN',
    '__NB_SUPPLY_MGMT_ADDR', '__NB_SUPPLY_MGMT_FEED', '__NB_SUPPLY_SIGNAL_FEED',
    '__NB_SUPPLY_ICE_OPEN', '__NB_SUPPLY_ICE_FEED'];
  const resetSupply = (): void => {
    for (const key of SUPPLY_KEYS) { G[key] = undefined; }
    G['__NB_SUPPLY_REPLY'] = undefined;
    G['__NB_SUPPLY_REPLY_LAST'] = undefined;
  };
  const supplyOf = (key: string): number => Number(G[key] ?? 0);
  const supplyTotal = (): number => {
    let total: number = 0;
    for (const key of SUPPLY_KEYS) { total += supplyOf(key); }
    return total;
  };
  const supplyLine = (): string => JSON.stringify({
    dns: supplyOf('__NB_SUPPLY_DNS'), resolve: supplyOf('__NB_SUPPLY_DNS_RESOLVE'),
    wgOpen: supplyOf('__NB_SUPPLY_WG_OPEN'), wgFeed: supplyOf('__NB_SUPPLY_WG_FEED'),
    mgmtOpen: supplyOf('__NB_SUPPLY_MGMT_OPEN'), mgmtAddr: supplyOf('__NB_SUPPLY_MGMT_ADDR'),
    mgmtFeed: supplyOf('__NB_SUPPLY_MGMT_FEED'), signalFeed: supplyOf('__NB_SUPPLY_SIGNAL_FEED'),
    iceOpen: supplyOf('__NB_SUPPLY_ICE_OPEN'), iceFeed: supplyOf('__NB_SUPPLY_ICE_FEED')
  });
  const sleepMs = async (ms: number): Promise<void> => {
    await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, ms); });
  };
  // Cancel the ability's own real timers, so a later 4 s ICE top-up tick can
  // never land inside another scenario's supply counts.
  const stopAbilityTimers = (ability: Record<string, unknown>): void => {
    const ice: unknown = ability['iceTopUpTimer'];
    if (typeof ice === 'number' && ice !== -1) { clearInterval(ice); }
    ability['iceTopUpTimer'] = -1;
    const supply: unknown = ability['socketSupplyTimer'];
    if (typeof supply === 'number' && supply !== -1) { clearInterval(supply); }
    ability['socketSupplyTimer'] = -1;
  };
  const controlCalls = (): number => Number(G['__NB_CONTROL_CALLS'] ?? 0);
  const armedAbility = (): Record<string, unknown> => {
    const ability = freshAbility(mod, FIXTURE_FILESDIR);
    ability['n2Evidence'] = N2_EVIDENCE_OPTIONS;
    return ability;
  };

  // N1: the explicit non-secret want lands byte-verbatim in filesDir, logged
  // as file name + real UTF-8 byte count, payload never printed.
  resetFs();
  const controlEntry = newEntry();
  resetLog();
  (controlEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_control': CONTROL_PAYLOAD } }, {});
  check('N1 the control want payload is written VERBATIM to <filesDir>/netbird-n2-control',
    fileOf(CONTROL_PATH) === CONTROL_PAYLOAD, JSON.stringify(fileOf(CONTROL_PATH)));
  check('N1 the write is logged as file name + UTF-8 byte count only',
    linesWith('N2_CONTROL_FILE_WRITTEN|').length === 1 &&
    lastLine('N2_CONTROL_FILE_WRITTEN|') === 'N2_CONTROL_FILE_WRITTEN|file=' + CONTROL_NAME +
    '|bytes=' + String(Buffer.byteLength(CONTROL_PAYLOAD, 'utf8')) +
    '|note=diagnostic-only-non-secret',
    linesWith('N2_CONTROL_FILE_WRITTEN|').join('\n'));
  check('N1 the payload never reaches the log (no target string in any line)',
    logs().join('\n').indexOf('119.29.29.119') < 0, JSON.stringify(logs()));

  // N2: a launch WITHOUT the parameter deletes a leftover control request (the
  // structural default-off rule the arm ticket and the stimulus share).
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  resetLog();
  (controlEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('N2 a launch without n2_control deletes the leftover request',
    fileOf(CONTROL_PATH) === undefined &&
    lastLine('N2_CONTROL_FILE_REMOVED|') === 'N2_CONTROL_FILE_REMOVED|file=' + CONTROL_NAME +
    '|note=default-no-control',
    JSON.stringify({ file: fileOf(CONTROL_PATH), log: lastLine('N2_CONTROL_FILE_REMOVED|') }));

  // N2b: the control-only launch also clears a STIMULUS leftover — one launch
  // carries at most ONE pending request, so an older request can never fire
  // behind the operator's back.
  seedFile(STIMULUS_PATH, STIMULUS_PAYLOAD);
  resetLog();
  (controlEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_control': CONTROL_PAYLOAD } }, {});
  check('N2b a control-only launch clears the stimulus leftover and writes only the control',
    fileOf(STIMULUS_PATH) === undefined && fileOf(CONTROL_PATH) === CONTROL_PAYLOAD &&
    lastLine('N2_STIMULUS_FILE_REMOVED|') === 'N2_STIMULUS_FILE_REMOVED|file=netbird-n2-stimulus' +
    '|note=default-no-stimulus',
    JSON.stringify({ stimulus: fileOf(STIMULUS_PATH), control: fileOf(CONTROL_PATH) }));

  // N3: a SINGLE launch carrying both parameters writes NEITHER (the
  // combination is refused, not ordered) and removes both leftovers.
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  seedFile(STIMULUS_PATH, STIMULUS_PAYLOAD);
  resetLog();
  (controlEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD,
        'n2_stimulus': STIMULUS_PAYLOAD, 'n2_control': CONTROL_PAYLOAD } }, {});
  check('N3 one launch carrying BOTH requests writes NEITHER and removes both leftovers',
    fileOf(CONTROL_PATH) === undefined && fileOf(STIMULUS_PATH) === undefined &&
    linesWith('N2_CONTROL_FILE_WRITTEN|').length === 0 &&
    linesWith('N2_STIMULUS_FILE_WRITTEN|').length === 0,
    JSON.stringify({ control: fileOf(CONTROL_PATH), stimulus: fileOf(STIMULUS_PATH) }));
  check('N3 the refusal is one explicit line naming both files',
    linesWith('N2_CONTROL_COMBO_REJECTED|').length === 1 &&
    lastLine('N2_CONTROL_COMBO_REJECTED|') === 'N2_CONTROL_COMBO_REJECTED|stimulus=netbird-n2-stimulus' +
    '|control=' + CONTROL_NAME + '|note=one-pending-request-per-launch|diag=true|is_evidence=false',
    lastLine('N2_CONTROL_COMBO_REJECTED|'));

  // N3b (T7a item 7): the ARM TICKET is protected across a request launch. The
  // documented executor flow is "arm once, then one launch per request", so a
  // launch that carries an N2 request but NO arm ticket must KEEP the existing
  // ticket — deleting it would leave the already-running extension armed but a
  // later extension restart unarmed (the arm would be silently lost).
  seedFile(ARM_TICKET_PATH, ARM_PAYLOAD);
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  resetLog();
  (controlEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2_control': CONTROL_PAYLOAD } }, {});
  check('N3b a request launch WITHOUT the arm ticket keeps the ticket and logs KEPT',
    fileOf(ARM_TICKET_PATH) === ARM_PAYLOAD && fileOf(CONTROL_PATH) === CONTROL_PAYLOAD &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 0 &&
    linesWith('N2H_ARM_TICKET_WRITTEN|').length === 0 &&
    lastLine('N2H_ARM_TICKET_KEPT|') === 'N2H_ARM_TICKET_KEPT|file=' + ARM_TICKET_NAME +
    '|note=request-launch-without-arm-ticket|diag=true|is_evidence=false',
    JSON.stringify({ arm: fileOf(ARM_TICKET_PATH), kept: lastLine('N2H_ARM_TICKET_KEPT|') }));
  // …the SAME rule for the other request channels, and the ORIGINAL
  // default-no-arm rule still holds for a launch with no N2 request at all.
  seedFile(ARM_TICKET_PATH, ARM_PAYLOAD);
  resetLog();
  (controlEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2_stimulus': STIMULUS_PAYLOAD } }, {});
  check('N3b a stimulus launch without the arm ticket keeps the ticket too',
    fileOf(ARM_TICKET_PATH) === ARM_PAYLOAD && fileOf(STIMULUS_PATH) === STIMULUS_PAYLOAD &&
    linesWith('N2H_ARM_TICKET_KEPT|').length === 1 &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 0,
    JSON.stringify({ arm: fileOf(ARM_TICKET_PATH), logs: linesWith('N2H_ARM_TICKET_KEPT|') }));
  seedFile(ARM_TICKET_PATH, ARM_PAYLOAD);
  resetLog();
  (controlEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird' }, {});
  check('N3b a launch with no N2 request at all still clears the ticket (default: no arm)',
    fileOf(ARM_TICKET_PATH) === undefined &&
    linesWith('N2H_ARM_TICKET_KEPT|').length === 0 &&
    lastLine('N2H_ARM_TICKET_REMOVED|') === 'N2H_ARM_TICKET_REMOVED|file=' + ARM_TICKET_NAME +
    '|note=default-no-arm',
    JSON.stringify({ arm: fileOf(ARM_TICKET_PATH), logs: logs() }));

  // N4: the ARMED poll consumes the request once and calls the
  // positive-control entry with exactly the parsed target/proto/id.
  resetFs();
  resetRequestState();
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  const armedForControl = armedAbility();
  resetLog();
  (armedForControl['pollN2Control'] as () => void)();
  const controlLast = (G['__NB_CONTROL_LAST'] ?? {}) as Record<string, unknown>;
  check('N4 the armed poll calls the positive-control entry exactly ONCE with the parsed request',
    controlCalls() === 1 && controlLast['armed'] === N2_EVIDENCE_OPTIONS &&
    controlLast['target'] === '119.29.29.119:53' && controlLast['protocol'] === 'udp' &&
    controlLast['id'] === 'evt1-pre', JSON.stringify(controlLast));
  check('N4 the shell receipt says sent and the request is consumed exactly once',
    lastLine('N2_CONTROL_SHELL|') === 'N2_CONTROL_SHELL|requestId=' + REQUEST_ID +
    '|id=evt1-pre|target=119.29.29.119:53|proto=udp|result=sent|reason=ok|diag=true|is_evidence=false' &&
    fileOf(CONTROL_PATH) === undefined,
    lastLine('N2_CONTROL_SHELL|'));

  // N5: an UNARMED session refuses by name, never calls the core, and still
  // consumes the file (nothing can fire after a later arming).
  resetFs();
  resetRequestState();
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  const unarmedAbility = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  (unarmedAbility['pollN2Control'] as () => void)();
  check('N5 an unarmed session refuses (not-armed), calls the core ZERO times, and consumes the file',
    controlCalls() === 0 && fileOf(CONTROL_PATH) === undefined &&
    lastLine('N2_CONTROL_SHELL|') === 'N2_CONTROL_SHELL|requestId=' + REQUEST_ID +
    '|id=evt1-pre|target=119.29.29.119:53|proto=udp|result=rejected|reason=not-armed|' +
    'diag=true|is_evidence=false',
    JSON.stringify({ calls: controlCalls(), line: lastLine('N2_CONTROL_SHELL|') }));

  // N6: an armed send refusal (the shell's capture-32 check / a core error) is
  // REJECTED with its own reason — never reported as sent.
  resetFs();
  resetRequestState();
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  const sendRefusedAbility = armedAbility();
  G['__NB_CONTROL_MODE'] = 'throw';
  G['__NB_CONTROL_ERROR'] = 'target-not-captured';
  resetLog();
  (sendRefusedAbility['pollN2Control'] as () => void)();
  check('N6 an armed send refusal is rejected with its reason and the file is consumed',
    controlCalls() === 1 && fileOf(CONTROL_PATH) === undefined &&
    lastLine('N2_CONTROL_SHELL|') === 'N2_CONTROL_SHELL|requestId=' + REQUEST_ID +
    '|id=evt1-pre|target=119.29.29.119:53|proto=udp|result=rejected|reason=target-not-captured|' +
    'diag=true|is_evidence=false',
    lastLine('N2_CONTROL_SHELL|'));

  // N7: a malformed payload is refused by name BEFORE any core call.
  resetFs();
  resetRequestState();
  seedFile(CONTROL_PATH, '{"target":"119.29.29.119:53","proto":"sctp","id":"x"}');
  const malformedAbility = armedAbility();
  resetLog();
  (malformedAbility['pollN2Control'] as () => void)();
  check('N7 a malformed request is refused by name with no core call and is consumed',
    controlCalls() === 0 && fileOf(CONTROL_PATH) === undefined &&
    lastLine('N2_CONTROL_SHELL|') === 'N2_CONTROL_SHELL|requestId=' + REQUEST_ID +
    '|id=none|target=none|proto=none|result=rejected|reason=invalid-protocol|' +
    'diag=true|is_evidence=false',
    JSON.stringify({ calls: controlCalls(), line: lastLine('N2_CONTROL_SHELL|') }));

  // N8: the COMBINATION RULE through the REAL watcher tick — two separate
  // launches left both files pending, and one tick submits the CONTROL before
  // the stimulus, consuming both exactly once.
  resetFs();
  resetRequestState();
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  seedFile(STIMULUS_PATH, STIMULUS_PAYLOAD);
  G['__NB_STATUS'] = watcherStatus;
  G['__NB_CONTROL_MODE'] = 'sent';
  G['__NB_STIMULUS_ANSWER'] = JSON.stringify({ ok: true, result: 'applied',
    reason: 'shutdown-socket', target_fd: 31, remote: '203.0.113.7:443' });
  const orderAbility = armedAbility();
  resetLog();
  const orderTick = watcherTick(orderAbility);
  orderTick();
  const controlAt: number = logs().findIndex((l: string): boolean => l.indexOf('N2_CONTROL_SHELL|') >= 0);
  const stimulusAt: number = logs().findIndex((l: string): boolean => l.indexOf('N2_STIMULUS_SHELL|') >= 0);
  check('N8 one tick consumes BOTH pending requests (one line each)',
    linesWith('N2_CONTROL_SHELL|').length === 1 && linesWith('N2_STIMULUS_SHELL|').length === 1 &&
    fileOf(CONTROL_PATH) === undefined && fileOf(STIMULUS_PATH) === undefined,
    JSON.stringify(linesWith('N2_CONTROL_SHELL|').concat(linesWith('N2_STIMULUS_SHELL|'))));
  check('N8 within one tick the CONTROL is submitted BEFORE the stimulus',
    controlAt >= 0 && stimulusAt >= 0 && controlAt < stimulusAt,
    JSON.stringify({ controlAt, stimulusAt }));

  // ---------------- N9-N15 (N2 P4/P5, 2026-09-28): the A-column trigger
  // ---------------- private channel and the automatic I/Ws sandwich polls.
  //
  // SAME three-file mechanism as the arm ticket / stimulus / control, now with
  // a THIRD pending-request channel: EntryAbility writes
  // <filesDir>/netbird-n2-a-event from the non-secret `n2_a_event` want
  // parameter, the REAL extension's `pollN2AEvent` consumes it once through
  // the recording core seam, and the REAL watcher tick drives the sandwich
  // poll/post step. The core's own `N2_A_EVENT|…` / `N2_SANDWICH|…` lines stay
  // authoritative.
  const AEVENT_PATH = `${FIXTURE_FILESDIR}/netbird-n2-a-event`;
  const AEVENT_NAME = 'netbird-n2-a-event';
  const AEVENT_PAYLOAD = '{"kind":"M","id":"a-M-1"}';
  const aEventCalls = (): number => Number(G['__NB_A_EVENT_CALLS'] ?? 0);

  // N9: the explicit non-secret want lands byte-verbatim in filesDir, logged
  // as file name + real UTF-8 byte count, payload never printed.
  resetFs();
  const aEventEntry = newEntry();
  resetLog();
  (aEventEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_a_event': AEVENT_PAYLOAD } }, {});
  check('N9 the A-event want payload is written VERBATIM to <filesDir>/netbird-n2-a-event',
    fileOf(AEVENT_PATH) === AEVENT_PAYLOAD, JSON.stringify(fileOf(AEVENT_PATH)));
  check('N9 the write is logged as file name + UTF-8 byte count only',
    linesWith('N2_A_EVENT_FILE_WRITTEN|').length === 1 &&
    lastLine('N2_A_EVENT_FILE_WRITTEN|') === 'N2_A_EVENT_FILE_WRITTEN|file=' + AEVENT_NAME +
    '|bytes=' + String(Buffer.byteLength(AEVENT_PAYLOAD, 'utf8')) +
    '|note=diagnostic-only-non-secret' &&
    logs().join('\n').indexOf('a-M-1') < 0,
    linesWith('N2_A_EVENT_FILE_WRITTEN|').join('\n'));

  // N10: an a-event-only launch clears the OTHER two leftovers — one launch
  // carries at most one pending request.
  seedFile(STIMULUS_PATH, STIMULUS_PAYLOAD);
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  resetLog();
  (aEventEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_a_event': AEVENT_PAYLOAD } }, {});
  check('N10 an a-event-only launch clears the stimulus and control leftovers',
    fileOf(STIMULUS_PATH) === undefined && fileOf(CONTROL_PATH) === undefined &&
    fileOf(AEVENT_PATH) === AEVENT_PAYLOAD &&
    lastLine('N2_STIMULUS_FILE_REMOVED|') === 'N2_STIMULUS_FILE_REMOVED|file=netbird-n2-stimulus' +
    '|note=default-no-stimulus' &&
    lastLine('N2_CONTROL_FILE_REMOVED|') === 'N2_CONTROL_FILE_REMOVED|file=' + CONTROL_NAME +
    '|note=default-no-control',
    JSON.stringify({ stimulus: fileOf(STIMULUS_PATH), control: fileOf(CONTROL_PATH) }));

  // N10b: a launch without the parameter clears a leftover A-event request
  // (the structural default-off rule every channel shares).
  seedFile(AEVENT_PATH, AEVENT_PAYLOAD);
  resetLog();
  (aEventEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2h_arm_options': ARM_PAYLOAD } }, {});
  check('N10b a launch without n2_a_event deletes the leftover request',
    fileOf(AEVENT_PATH) === undefined &&
    lastLine('N2_A_EVENT_FILE_REMOVED|') === 'N2_A_EVENT_FILE_REMOVED|file=' + AEVENT_NAME +
    '|note=default-no-a-event',
    JSON.stringify({ file: fileOf(AEVENT_PATH), log: lastLine('N2_A_EVENT_FILE_REMOVED|') }));

  // N11: a launch carrying TWO of the three requests still writes NEITHER; the
  // refusal line now names the third file.
  seedFile(AEVENT_PATH, AEVENT_PAYLOAD);
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  resetLog();
  (aEventEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_control': CONTROL_PAYLOAD,
        'n2_a_event': AEVENT_PAYLOAD } }, {});
  check('N11 control + a_event write NEITHER and remove both leftovers',
    fileOf(AEVENT_PATH) === undefined && fileOf(CONTROL_PATH) === undefined &&
    linesWith('N2_A_EVENT_FILE_WRITTEN|').length === 0 &&
    linesWith('N2_CONTROL_FILE_WRITTEN|').length === 0);
  check('N11 the extended refusal names all three request files',
    linesWith('N2_CONTROL_COMBO_REJECTED|').length === 1 &&
    lastLine('N2_CONTROL_COMBO_REJECTED|') === 'N2_CONTROL_COMBO_REJECTED|stimulus=netbird-n2-stimulus' +
    '|control=' + CONTROL_NAME + '|a_event=' + AEVENT_NAME +
    '|note=one-pending-request-per-launch|diag=true|is_evidence=false',
    lastLine('N2_CONTROL_COMBO_REJECTED|'));

  // N12: the ARMED poll consumes the request once and calls the core with
  // exactly the parsed kind/id, then logs its own delivery receipt.
  resetFs();
  resetRequestState();
  seedFile(AEVENT_PATH, AEVENT_PAYLOAD);
  G['__NB_A_EVENT_ANSWER'] = JSON.stringify({ ok: true, result: 'sent', reason: 'ok',
    kind: 'M', fd: 21, dup_fd: 22, line: 'N2_A_EVENT|id=a-M-1|kind=M|result=sent' });
  const armedForAEvent = armedAbility();
  resetLog();
  (armedForAEvent['pollN2AEvent'] as () => void)();
  check('N12 the armed poll calls the core exactly ONCE with the parsed request',
    aEventCalls() === 1 &&
    JSON.stringify(G['__NB_A_EVENT_LAST']) === JSON.stringify({ kind: 'M', id: 'a-M-1' }),
    JSON.stringify(G['__NB_A_EVENT_LAST']));
  check('N12 the shell receipt names the core result, the fd pair, and consumes the file',
    lastLine('N2_A_EVENT_SHELL|') === 'N2_A_EVENT_SHELL|requestId=' + REQUEST_ID +
    '|id=a-M-1|kind=M|core_ok=true|core_result=sent|reason=ok|fd=21|dup_fd=22|diag=true|is_evidence=false' &&
    fileOf(AEVENT_PATH) === undefined,
    lastLine('N2_A_EVENT_SHELL|'));

  // N13: an UNARMED session refuses by name, never calls the core, and still
  // consumes the file (nothing can fire after a later arming).
  resetFs();
  resetRequestState();
  seedFile(AEVENT_PATH, AEVENT_PAYLOAD);
  const unarmedForAEvent = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  (unarmedForAEvent['pollN2AEvent'] as () => void)();
  check('N13 an unarmed session refuses (not-armed), calls the core ZERO times, consumes the file',
    aEventCalls() === 0 && fileOf(AEVENT_PATH) === undefined &&
    lastLine('N2_A_EVENT_SHELL|') === 'N2_A_EVENT_SHELL|requestId=' + REQUEST_ID +
    '|id=a-M-1|kind=M|core_ok=false|core_result=rejected|reason=not-armed|' +
    'fd=-1|dup_fd=-1|diag=true|is_evidence=false',
    JSON.stringify({ calls: aEventCalls(), line: lastLine('N2_A_EVENT_SHELL|') }));

  // N14: a malformed payload (an unknown kind) is refused by name BEFORE any
  // core call and is consumed.
  resetFs();
  resetRequestState();
  seedFile(AEVENT_PATH, '{"kind":"W","id":"x"}');
  const malformedForAEvent = armedAbility();
  resetLog();
  (malformedForAEvent['pollN2AEvent'] as () => void)();
  check('N14 a malformed request is refused by name with no core call and is consumed',
    aEventCalls() === 0 && fileOf(AEVENT_PATH) === undefined &&
    lastLine('N2_A_EVENT_SHELL|') === 'N2_A_EVENT_SHELL|requestId=' + REQUEST_ID +
    '|id=none|kind=none|core_ok=false|core_result=rejected|reason=invalid-kind|' +
    'fd=-1|dup_fd=-1|diag=true|is_evidence=false',
    JSON.stringify({ calls: aEventCalls(), line: lastLine('N2_A_EVENT_SHELL|') }));

  // N15: the sandwich polls ride the REAL armed watcher tick. Nothing ready =
  // one bare core poll, no control send; a ready leg = that leg's POST control
  // plus the core verdict report.
  resetFs();
  resetRequestState();
  G['__NB_STATUS'] = watcherStatus;
  const sandwichAbility = armedAbility();
  resetLog();
  const sandwichTick = watcherTick(sandwichAbility);
  sandwichTick();
  await drainProbe();
  await drainProbe();
  check('N15 an armed tick polls the sandwich core surface once and sends NO control',
    Number(G['__NB_SANDWICH_POLL_CALLS'] ?? 0) === 1 && controlCalls() === 0 &&
    Number(G['__NB_SANDWICH_POST_CALLS'] ?? 0) === 0,
    JSON.stringify({ poll: G['__NB_SANDWICH_POLL_CALLS'], controls: controlCalls() }));
  G['__NB_SANDWICH_POLL_ANSWER'] = JSON.stringify({ ok: true,
    posts: [{ kind: 'I', marker: 'n2s-' + REQUEST_ID + '-I-pre' }] });
  sandwichTick();
  await drainProbe();
  await drainProbe();
  const postLast = (G['__NB_SANDWICH_POST_LAST'] ?? {}) as Record<string, unknown>;
  check('N15 a ready leg sends its POST control and reports the verdict to the core',
    controlCalls() === 1 &&
    JSON.stringify(G['__NB_CONTROL_LAST']) === JSON.stringify({ armed: N2_EVIDENCE_OPTIONS,
      target: '119.29.29.119:53', protocol: 'udp', id: 'n2s-' + REQUEST_ID + '-I-pre' }) &&
    postLast['kind'] === 'I' && postLast['marker'] === 'n2s-' + REQUEST_ID + '-I-pre' &&
    postLast['result'] === 'observed' && postLast['reason'] === 'post-control-hit',
    JSON.stringify({ control: G['__NB_CONTROL_LAST'], post: postLast }));
  check('N15 the post step logs its own shell receipt',
    lastLine('N2_SANDWICH_SHELL|') === 'N2_SANDWICH_SHELL|kind=I|phase=post|marker=n2s-' +
    REQUEST_ID + '-I-pre|target=119.29.29.119:53|result=observed|reason=post-control-hit|' +
    'diag=true|is_evidence=false',
    lastLine('N2_SANDWICH_SHELL|'));

  // ---------------- N16-N25 (N2 first batch C/D columns, 2026-09-29) --------
  // The App-side switching sandwich and the app-internal evidence ring, driven
  // through the REAL EntryAbility (the 5-channel rule) and the REAL armed
  // extension polls against the recording core seams.
  const CD_PATH = `${FIXTURE_FILESDIR}/netbird-n2-cd-plan`;
  const CD_NAME = 'netbird-n2-cd-plan';
  const RING_DUMP_PATH = `${FIXTURE_FILESDIR}/netbird-n2-ring-dump`;
  const RING_DUMP_NAME = 'netbird-n2-ring-dump';
  // 2026-09-29 (explicit host ACK): the SEPARATE channel that alone may advance
  // the retrieved watermark.
  const RING_ACK_PATH = `${FIXTURE_FILESDIR}/netbird-n2-ring-ack`;
  const RING_ACK_NAME = 'netbird-n2-ring-ack';
  const RING_ACK_PAYLOAD = '{"session":"auto-1-2-3","up_to_seq":9}';
  const RING_PATH = `${FIXTURE_FILESDIR}/netbird-n2-evidence-ring.log`;
  const CD_PAYLOAD = '{"column":"C","kind":"M","rep":1,"id":"cd-C-M-1"}';
  const RING_DUMP_PAYLOAD = '{"from_seq":0,"to_seq":9,"count":2}';
  const cdEntry = newEntry();

  // N16: the explicit non-secret want lands byte-verbatim in filesDir, logged as
  // file name + real UTF-8 byte count, and clears the other five leftovers.
  resetFs();
  resetRequestState();
  seedFile(STIMULUS_PATH, STIMULUS_PAYLOAD);
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  seedFile(AEVENT_PATH, AEVENT_PAYLOAD);
  seedFile(RING_DUMP_PATH, RING_DUMP_PAYLOAD);
  seedFile(RING_ACK_PATH, RING_ACK_PAYLOAD);
  resetLog();
  (cdEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_cd_plan': CD_PAYLOAD } }, {});
  check('N16 the plan want payload is written VERBATIM to <filesDir>/netbird-n2-cd-plan',
    fileOf(CD_PATH) === CD_PAYLOAD, JSON.stringify(fileOf(CD_PATH)));
  check('N16 the write is logged as file name + UTF-8 byte count only, payload never printed',
    linesWith('N2_CD_PLAN_FILE_WRITTEN|').length === 1 &&
    lastLine('N2_CD_PLAN_FILE_WRITTEN|') === 'N2_CD_PLAN_FILE_WRITTEN|file=' + CD_NAME +
    '|bytes=' + String(Buffer.byteLength(CD_PAYLOAD, 'utf8')) +
    '|note=diagnostic-only-non-secret' &&
    logs().join('\n').indexOf('cd-C-M-1') < 0,
    linesWith('N2_CD_PLAN_FILE_WRITTEN|').join('\n'));
  check('N16 a plan-only launch clears the other five channel leftovers',
    fileOf(STIMULUS_PATH) === undefined && fileOf(CONTROL_PATH) === undefined &&
    fileOf(AEVENT_PATH) === undefined && fileOf(RING_DUMP_PATH) === undefined &&
    fileOf(RING_ACK_PATH) === undefined &&
    lastLine('N2_STIMULUS_FILE_REMOVED|') === 'N2_STIMULUS_FILE_REMOVED|file=netbird-n2-stimulus' +
    '|note=default-no-stimulus' &&
    lastLine('N2_A_EVENT_FILE_REMOVED|') === 'N2_A_EVENT_FILE_REMOVED|file=' + AEVENT_NAME +
    '|note=default-no-a-event' &&
    lastLine('N2_RING_DUMP_FILE_REMOVED|') === 'N2_RING_DUMP_FILE_REMOVED|file=' + RING_DUMP_NAME +
    '|note=default-no-ring-dump' &&
    lastLine('N2_RING_ACK_FILE_REMOVED|') === 'N2_RING_ACK_FILE_REMOVED|file=' + RING_ACK_NAME +
    '|note=default-no-ring-ack',
    JSON.stringify({ stimulus: fileOf(STIMULUS_PATH), control: fileOf(CONTROL_PATH),
      aEvent: fileOf(AEVENT_PATH), ringDump: fileOf(RING_DUMP_PATH),
      ringAck: fileOf(RING_ACK_PATH) }));

  // N16b: the arm ticket is KEPT for a plan-only launch (the T7a item 7 rule now
  // spans the six request channels).
  seedFile(ARM_TICKET_PATH, ARM_PAYLOAD);
  resetLog();
  (cdEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird', parameters: { 'n2_cd_plan': CD_PAYLOAD } }, {});
  check('N16b a plan launch WITHOUT the arm ticket keeps the ticket and logs KEPT',
    fileOf(ARM_TICKET_PATH) === ARM_PAYLOAD && fileOf(CD_PATH) === CD_PAYLOAD &&
    linesWith('N2H_ARM_TICKET_REMOVED|').length === 0 &&
    lastLine('N2H_ARM_TICKET_KEPT|') === 'N2H_ARM_TICKET_KEPT|file=' + ARM_TICKET_NAME +
    '|note=request-launch-without-arm-ticket|diag=true|is_evidence=false',
    JSON.stringify({ arm: fileOf(ARM_TICKET_PATH), kept: lastLine('N2H_ARM_TICKET_KEPT|') }));

  // N17: the one-pending-request-per-launch rule now spans SIX channels. A
  // combination the pre-existing branches do not cover names exactly the
  // pending channels; the legacy two/three-channel shapes keep their exact
  // lines (N3/N11 above).
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  seedFile(CONTROL_PATH, CONTROL_PAYLOAD);
  resetLog();
  (cdEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_cd_plan': CD_PAYLOAD,
        'n2_control': CONTROL_PAYLOAD } }, {});
  check('N17 a plan + control launch writes NEITHER and removes both leftovers',
    fileOf(CD_PATH) === undefined && fileOf(CONTROL_PATH) === undefined &&
    linesWith('N2_CD_PLAN_FILE_WRITTEN|').length === 0 &&
    linesWith('N2_CONTROL_FILE_WRITTEN|').length === 0);
  check('N17 the refusal names exactly the pending channels by file',
    linesWith('N2_CONTROL_COMBO_REJECTED|').length === 1 &&
    lastLine('N2_CONTROL_COMBO_REJECTED|') === 'N2_CONTROL_COMBO_REJECTED|pending=' +
    CONTROL_NAME + ',' + CD_NAME + '|note=one-pending-request-per-launch|diag=true|is_evidence=false',
    lastLine('N2_CONTROL_COMBO_REJECTED|'));

  // N17b: a ring-dump-only launch writes its own file and clears the others.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  resetLog();
  (cdEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_ring_dump': RING_DUMP_PAYLOAD } }, {});
  check('N17b a ring-dump-only launch writes the dump request and clears the plan leftover',
    fileOf(RING_DUMP_PATH) === RING_DUMP_PAYLOAD && fileOf(CD_PATH) === undefined &&
    lastLine('N2_RING_DUMP_FILE_WRITTEN|') === 'N2_RING_DUMP_FILE_WRITTEN|file=' + RING_DUMP_NAME +
    '|bytes=' + String(Buffer.byteLength(RING_DUMP_PAYLOAD, 'utf8')) +
    '|note=diagnostic-only-non-secret' &&
    lastLine('N2_CD_PLAN_FILE_REMOVED|') === 'N2_CD_PLAN_FILE_REMOVED|file=' + CD_NAME +
    '|note=default-no-cd-plan',
    JSON.stringify({ ringDump: fileOf(RING_DUMP_PATH), plan: fileOf(CD_PATH) }));

  // N17c: a ring-ACK-only launch writes its own file and clears the others — a
  // launch carrying BOTH ring channels writes NEITHER (the generic refusal names
  // exactly the pending files, in the fixed channel order).
  resetFs();
  resetRequestState();
  seedFile(RING_DUMP_PATH, RING_DUMP_PAYLOAD);
  seedFile(CD_PATH, CD_PAYLOAD);
  resetLog();
  (cdEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_ring_ack': RING_ACK_PAYLOAD } }, {});
  check('N17c a ring-ACK-only launch writes the ACK request and clears the other leftovers',
    fileOf(RING_ACK_PATH) === RING_ACK_PAYLOAD && fileOf(RING_DUMP_PATH) === undefined &&
    fileOf(CD_PATH) === undefined &&
    lastLine('N2_RING_ACK_FILE_WRITTEN|') === 'N2_RING_ACK_FILE_WRITTEN|file=' + RING_ACK_NAME +
    '|bytes=' + String(Buffer.byteLength(RING_ACK_PAYLOAD, 'utf8')) +
    '|note=diagnostic-only-non-secret' &&
    lastLine('N2_RING_DUMP_FILE_REMOVED|') === 'N2_RING_DUMP_FILE_REMOVED|file=' + RING_DUMP_NAME +
    '|note=default-no-ring-dump',
    JSON.stringify({ ringAck: fileOf(RING_ACK_PATH), ringDump: fileOf(RING_DUMP_PATH),
      plan: fileOf(CD_PATH) }));
  resetFs();
  resetRequestState();
  seedFile(RING_DUMP_PATH, RING_DUMP_PAYLOAD);
  resetLog();
  (cdEntry['onCreate'] as (w: unknown, l: unknown) => void)(
    { bundleName: 'cn.alfadb.netbird',
      parameters: { 'n2h_arm_options': ARM_PAYLOAD, 'n2_ring_dump': RING_DUMP_PAYLOAD,
        'n2_ring_ack': RING_ACK_PAYLOAD } }, {});
  check('N17c a dump + ACK launch writes NEITHER and names both pending files',
    fileOf(RING_DUMP_PATH) === undefined && fileOf(RING_ACK_PATH) === undefined &&
    linesWith('N2_RING_DUMP_FILE_WRITTEN|').length === 0 &&
    linesWith('N2_RING_ACK_FILE_WRITTEN|').length === 0 &&
    lastLine('N2_CONTROL_COMBO_REJECTED|') === 'N2_CONTROL_COMBO_REJECTED|pending=' +
    RING_DUMP_NAME + ',' + RING_ACK_NAME +
    '|note=one-pending-request-per-launch|diag=true|is_evidence=false',
    JSON.stringify({ ringDump: fileOf(RING_DUMP_PATH), ringAck: fileOf(RING_ACK_PATH),
      line: lastLine('N2_CONTROL_COMBO_REJECTED|') }));

  // N18: the ARMED plan poll measures the starting bearer, hands the parsed plan
  // to the core with the shell's verdict and target and logs its plan receipt.
  // The NEW orchestration sends NO control here: acceptance only REGISTERS the
  // plan (both controls belong to the plan's own network — the pre is owed on
  // the NEW network after the formal switch). The bearer facts need the SAME
  // handle `physicalDefaultNetId()` selected: a NOT_VPN capability (15) and at
  // least one IPv4 resolver, plus the net in `getAllNetsSync`.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  seedNetCtx({
    __NB_NET_DEFAULT_ID: 101,
    __NB_NET_ALL_IDS: [101],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] } },
    __NB_NET_PROPS_BY_ID: { 101: { dnses: [{ address: '119.29.29.29', family: 1 }],
      linkAddresses: [{ address: { address: '192.168.50.199', family: 1 } }] } }
  });
  G['__NB_CD_PLAN_ANSWER'] = JSON.stringify({ ok: true, result: 'planned', reason: 'ok',
    id: 'cd-C-M-1', pre_marker: 'cd-C-M-1-pre', post_marker: 'cd-C-M-1-post', line: '' });
  const cdAbility = armedAbility();
  resetLog();
  (cdAbility['pollN2CdPlan'] as () => void)();
  const cdPlanLast = (G['__NB_CD_PLAN_LAST'] ?? {}) as Record<string, unknown>;
  check('N18 the armed poll calls the core exactly ONCE with the parsed plan, the measured ' +
    'bearer facts, the tcp target of its kind and the shell start verdict',
    Number(G['__NB_CD_PLAN_CALLS'] ?? 0) === 1 && cdPlanLast['id'] === 'cd-C-M-1' &&
    cdPlanLast['column'] === 'C' && cdPlanLast['kind'] === 'M' && cdPlanLast['rep'] === 1 &&
    cdPlanLast['target'] === '119.29.29.119:443' && cdPlanLast['netBefore'] === 'wifi' &&
    cdPlanLast['localBefore'] === '192.168.50.199' && cdPlanLast['startVerdict'] === '',
    JSON.stringify(cdPlanLast));
  check('N18 the plan receipt names the marker the core handed back, and the file is consumed once',
    linesWith('N2_CD_SANDWICH_SHELL|').length === 1 &&
    linesWith('N2_CD_SANDWICH_SHELL|')[0] === 'N2_CD_SANDWICH_SHELL|requestId=' + REQUEST_ID +
    '|id=cd-C-M-1|column=C|kind=M|rep=1|phase=plan|marker=cd-C-M-1-pre|result=planned|reason=ok|' +
    'diag=true|is_evidence=false' && fileOf(CD_PATH) === undefined,
    JSON.stringify(linesWith('N2_CD_SANDWICH_SHELL|')));
  check('N18 acceptance sends NO control: the pre belongs to the NEW network, not the old one',
    controlCalls() === 0 && (G['__NB_CD_PRE_CALLS'] ?? 0) === 0 &&
    (G['__NB_CD_POLL_CALLS'] ?? 0) === 0 && cdAbility['n2CdActive'] === 'cd-C-M-1',
    JSON.stringify({ controls: controlCalls(), pre: G['__NB_CD_PRE_CALLS'],
      polls: G['__NB_CD_POLL_CALLS'], active: cdAbility['n2CdActive'] }));

  // N19: the SWITCH hook. The observer reports the plan switch BEFORE the
  // re-feed, with the direction verdict the pure module computed from the
  // measured bearer transition (column C: wifi -> cellular is the valid one)
  // AND the identity of the OLD network's live registration of the kind. The
  // core's own poll then drives the PRE control on the NEW network and, once
  // the tee HIT it, opens the recovery gate.
  seedNetCtx({
    __NB_NET_DEFAULT_ID: 202,
    __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: {
      101: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '192.168.50.199', family: 1 } }] },
      202: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '10.77.0.5', family: 1 } }] }
    }
  });
  cdAbility['networkNetId'] = 101;
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 31, dup_fd: 32,
    generation: 5, net_id: 101 });
  G['__NB_CD_POLL_ANSWER_QUEUE'] = [
    JSON.stringify({ ok: true, step: 'pre', phase: '', id: 'cd-C-M-1', kind: 'M', column: 'C',
      fd: -1, dup_fd: -1, remote: 'unknown', marker: 'cd-C-M-1-pre', result: 'pre',
      reason: 'send-pre-now', line: '' }),
    JSON.stringify({ ok: true, step: 'recover', phase: '', id: 'cd-C-M-1', kind: 'M',
      column: 'C', fd: -1, dup_fd: -1, remote: 'unknown', marker: '', result: 'recover',
      reason: 'pre-hit-observed', line: '' })
  ];
  resetLog();
  (cdAbility['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 202, 202);
  const cdSwitchLast = (G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>;
  check('N19 the identified switch report carries the after-facts, the new netId and the OLD ' +
    'identity triple',
    Number(G['__NB_CD_SWITCH_CALLS'] ?? 0) === 1 && cdSwitchLast['id'] === 'cd-C-M-1' &&
    cdSwitchLast['verdict'] === '' && cdSwitchLast['netAfter'] === 'cellular' &&
    cdSwitchLast['localAfter'] === '10.77.0.5' && cdSwitchLast['netId'] === 202 &&
    cdSwitchLast['fd'] === 31 && cdSwitchLast['dupFd'] === 32 &&
    cdSwitchLast['generation'] === 5,
    JSON.stringify(cdSwitchLast));
  check('N19 the identity baseline is read from the core for the plan\'s OWN kind',
    Number(G['__NB_CD_IDENTITY_CALLS'] ?? 0) === 1 &&
    JSON.stringify(G['__NB_CD_IDENTITY_LAST']) === JSON.stringify({ kind: 'M' }));
  check('N19 the switch receipt is a phase line of its own',
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=switch|marker=none|result=switched|reason=ok|') > 0,
    lastLine('N2_CD_SANDWICH_SHELL|'));
  check('N19 the switch evidence reaches the ring verbatim (same text as hilog)',
    ((G['__NB_RING'] as string[] ?? []).indexOf(lastLine('VPN_NETWORK_EVENT|')) >= 0) &&
    lastLine('VPN_NETWORK_EVENT|').indexOf('|switched=true|action=redial') > 0,
    JSON.stringify({ ring: G['__NB_RING'], log: lastLine('VPN_NETWORK_EVENT|') }));
  await drainProbe();
  await drainProbe();
  check('N19 the switch-instant step poll sends the PRE control on the NEW network and reports ' +
    'the tee\'s HIT to the core',
    controlCalls() === 1 &&
    JSON.stringify(G['__NB_CONTROL_LAST']) === JSON.stringify({ armed: N2_EVIDENCE_OPTIONS,
      target: '119.29.29.119:443', protocol: 'tcp', id: 'cd-C-M-1-pre' }) &&
    JSON.stringify(G['__NB_CD_PRE_LAST']) === JSON.stringify({ id: 'cd-C-M-1', result: 'hit',
      reason: 'ok' }),
    JSON.stringify({ control: G['__NB_CONTROL_LAST'], pre: G['__NB_CD_PRE_LAST'] }));
  check('N19 the core\'s step=recover opens the recovery gate exactly once',
    cdAbility['n2CdGateState'] === 'released' &&
    cdAbility['n2CdGatePlan'] === 'cd-C-M-1' &&
    cdAbility['n2CdGateEpoch'] === cdAbility['networkEpoch'] &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=pre|marker=cd-C-M-1-pre|result=hit|') > 0,
    JSON.stringify({ gate: cdAbility['n2CdGateState'],
      line: lastLine('N2_CD_SANDWICH_SHELL|') }));

  // N19b: the WRONG direction is refused by name. Column C expects
  // wifi -> cellular; the measured transition here is wifi -> wifi, so the
  // pure module answers `direction-mismatch`, the shell hands that token to the
  // core, and the terminal line says `unsat` with it.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  seedNetCtx({
    __NB_NET_DEFAULT_ID: 202,
    __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [1] } },
    __NB_NET_PROPS_BY_ID: {
      101: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '192.168.50.199', family: 1 } }] },
      202: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '192.168.50.240', family: 1 } }] }
    }
  });
  G['__NB_CD_PLAN_ANSWER'] = JSON.stringify({ ok: true, result: 'planned', reason: 'ok',
    id: 'cd-C-M-1', pre_marker: 'cd-C-M-1-pre', post_marker: 'cd-C-M-1-post', line: '' });
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 33, dup_fd: 34,
    generation: 5, net_id: 101 });
  G['__NB_CD_SWITCH_ANSWER'] = JSON.stringify({ ok: false, result: 'unsat',
    reason: 'direction-mismatch', id: 'cd-C-M-1', line: '' });
  const mismatchAbility = armedAbility();
  resetLog();
  (mismatchAbility['pollN2CdPlan'] as () => void)();
  await drainProbe();
  await drainProbe();
  mismatchAbility['networkNetId'] = 101;
  resetLog();
  (mismatchAbility['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 202, 202);
  check('N19b a wrong-direction transition is refused by name and passed to the core',
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['verdict'] ===
      'direction-mismatch' &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['fd'] === 33 &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['generation'] === 5 &&
    lastLine('N2_CD_SANDWICH_SHELL|') === 'N2_CD_SANDWICH_SHELL|requestId=' + REQUEST_ID +
    '|id=cd-C-M-1|column=C|kind=M|rep=1|phase=switch|marker=none|result=unsat|' +
    'reason=direction-mismatch|diag=true|is_evidence=false',
    JSON.stringify({ switch: G['__NB_CD_SWITCH_LAST'],
      line: lastLine('N2_CD_SANDWICH_SHELL|') }));
  check('N19b a terminal UNSAT drops the shell latch and sends NO control',
    mismatchAbility['n2CdActive'] === '' && controlCalls() === 0,
    JSON.stringify({ active: mismatchAbility['n2CdActive'], controls: controlCalls() }));

  // N20: an UNARMED session refuses the plan by name, calls the core ZERO times
  // and still consumes the file.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  const unarmedCd = freshAbility(mod, FIXTURE_FILESDIR);
  resetLog();
  (unarmedCd['pollN2CdPlan'] as () => void)();
  check('N20 an unarmed session refuses (not-armed), calls the core ZERO times, consumes the file',
    Number(G['__NB_CD_PLAN_CALLS'] ?? 0) === 0 && fileOf(CD_PATH) === undefined &&
    lastLine('N2_CD_SANDWICH_SHELL|') === 'N2_CD_SANDWICH_SHELL|requestId=' + REQUEST_ID +
    '|id=cd-C-M-1|column=C|kind=M|rep=1|phase=plan|marker=none|result=rejected|reason=not-armed|' +
    'diag=true|is_evidence=false',
    JSON.stringify({ calls: G['__NB_CD_PLAN_CALLS'], line: lastLine('N2_CD_SANDWICH_SHELL|') }));

  // N21: a malformed payload is refused by name BEFORE any core call.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, '{"column":"E","kind":"M","rep":1,"id":"x"}');
  const malformedCd = armedAbility();
  resetLog();
  (malformedCd['pollN2CdPlan'] as () => void)();
  check('N21 a malformed plan is refused by name with no core call and is consumed',
    Number(G['__NB_CD_PLAN_CALLS'] ?? 0) === 0 && fileOf(CD_PATH) === undefined &&
    lastLine('N2_CD_SANDWICH_SHELL|') === 'N2_CD_SANDWICH_SHELL|requestId=' + REQUEST_ID +
    '|id=none|column=none|kind=none|rep=0|phase=plan|marker=none|result=rejected|' +
    'reason=invalid-column|diag=true|is_evidence=false',
    JSON.stringify({ calls: G['__NB_CD_PLAN_CALLS'], line: lastLine('N2_CD_SANDWICH_SHELL|') }));

  // N21b: Wb is NOT exempt any more. The core's revised state machine owes the
  // base socket a PRE and a POST control like every other class; its RECEIPT is
  // the new generation's `protect_settle` and the switch hands out no
  // `exempted`. The shell half measured here: acceptance registers without a
  // control, the switch-instant poll sends the pre, the receipt step sends the
  // post — TWO controls, both on the new network, both reported to the core.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, '{"column":"C","kind":"Wb","rep":1,"id":"cd-C-Wb-1"}');
  seedNetCtx({ __NB_NET_DEFAULT_ID: 101, __NB_NET_ALL_IDS: [101],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] } },
    __NB_NET_PROPS_BY_ID: { 101: { dnses: [{ address: '119.29.29.29', family: 1 }],
      linkAddresses: [{ address: { address: '192.168.50.199', family: 1 } }] } } });
  G['__NB_CD_PLAN_ANSWER'] = JSON.stringify({ ok: true, result: 'planned', reason: 'ok',
    id: 'cd-C-Wb-1', pre_marker: 'cd-C-Wb-1-pre', post_marker: 'cd-C-Wb-1-post', line: '' });
  const wbAbility = armedAbility();
  resetLog();
  (wbAbility['pollN2CdPlan'] as () => void)();
  check('N21b a Wb plan is registered and acceptance still sends NO control',
    Number(G['__NB_CD_PLAN_CALLS'] ?? 0) === 1 && controlCalls() === 0 &&
    (G['__NB_CD_PRE_CALLS'] ?? 0) === 0 &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=plan|marker=cd-C-Wb-1-pre|result=planned|') > 0,
    JSON.stringify({ calls: G['__NB_CD_PLAN_CALLS'], controls: controlCalls(),
      pre: G['__NB_CD_PRE_CALLS'], line: lastLine('N2_CD_SANDWICH_SHELL|') }));
  seedNetCtx({ __NB_NET_DEFAULT_ID: 202, __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: {
      101: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '192.168.50.199', family: 1 } }] },
      202: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '10.77.0.5', family: 1 } }] } } });
  wbAbility['networkNetId'] = 101;
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 41, dup_fd: 42,
    generation: 2, net_id: 101 });
  G['__NB_CD_POLL_ANSWER_QUEUE'] = [
    JSON.stringify({ ok: true, step: 'pre', phase: '', id: 'cd-C-Wb-1', kind: 'Wb', column: 'C',
      fd: -1, dup_fd: -1, remote: 'unknown', marker: 'cd-C-Wb-1-pre', result: 'pre',
      reason: 'send-pre-now', line: '' }),
    JSON.stringify({ ok: true, step: 'receipt', phase: 'receipt', id: 'cd-C-Wb-1', kind: 'Wb',
      column: 'C', fd: 41, dup_fd: 42, remote: '192.0.2.9:53', marker: 'cd-C-Wb-1-post',
      result: 'exempted', reason: 'wb-protect-settle-success', line: '' })
  ];
  resetLog();
  (wbAbility['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 202, 202);
  check('N21b its switch is still measured and the plan switches with the identity baseline',
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['id'] === 'cd-C-Wb-1' &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['verdict'] === '' &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['netAfter'] === 'cellular' &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['fd'] === 41 &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=switch|marker=none|result=switched|') > 0,
    JSON.stringify({ switch: G['__NB_CD_SWITCH_LAST'],
      line: lastLine('N2_CD_SANDWICH_SHELL|') }));
  await drainProbe();
  await drainProbe();
  const wbPreLast = (G['__NB_CD_PRE_LAST'] ?? {}) as Record<string, unknown>;
  check('N21b the Wb row now gets its PRE control on the new network (udp, its own opener)',
    controlCalls() === 2 &&
    JSON.stringify(G['__NB_CONTROL_IDS']) === JSON.stringify(['cd-C-Wb-1-pre', 'cd-C-Wb-1-post']) &&
    wbPreLast['id'] === 'cd-C-Wb-1' && wbPreLast['result'] === 'hit',
    JSON.stringify({ ids: G['__NB_CONTROL_IDS'], pre: wbPreLast }));
  const wbPostLast = (G['__NB_CD_POST_LAST'] ?? {}) as Record<string, unknown>;
  check('N21b the Wb row now gets its POST control too: TWO controls, both reported to the core',
    controlCalls() === 2 &&
    JSON.stringify(G['__NB_CONTROL_LAST']) === JSON.stringify({ armed: N2_EVIDENCE_OPTIONS,
      target: '119.29.29.119:53', protocol: 'udp', id: 'cd-C-Wb-1-post' }) &&
    wbPostLast['id'] === 'cd-C-Wb-1' && wbPostLast['result'] === 'observed' &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=post|marker=cd-C-Wb-1-post|result=observed|') > 0,
    JSON.stringify({ controls: controlCalls(), control: G['__NB_CONTROL_LAST'], post: wbPostLast,
      line: lastLine('N2_CD_SANDWICH_SHELL|') }));

  // N22: the two control legs of ONE plan, in the new orchestration's order.
  // Acceptance registers only; the SWITCH-instant step poll sends the PRE on the
  // new network; the core's `step=receipt` then sends the POST with the marker
  // the CORE handed back. `step=closed` is a terminal line the core already
  // emitted (the wait expiries arrive this way) and only clears the shell latch.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  seedNetCtx({ __NB_NET_DEFAULT_ID: 101, __NB_NET_ALL_IDS: [101],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] } },
    __NB_NET_PROPS_BY_ID: { 101: { dnses: [{ address: '119.29.29.29', family: 1 }],
      linkAddresses: [{ address: { address: '192.168.50.199', family: 1 } }] } } });
  G['__NB_CD_PLAN_ANSWER'] = JSON.stringify({ ok: true, result: 'planned', reason: 'ok',
    id: 'cd-C-M-1', pre_marker: 'cd-C-M-1-pre', post_marker: 'cd-C-M-1-post', line: '' });
  const stepAbility = armedAbility();
  resetLog();
  (stepAbility['pollN2CdPlan'] as () => void)();
  seedNetCtx({ __NB_NET_DEFAULT_ID: 202, __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: {
      101: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '192.168.50.199', family: 1 } }] },
      202: { dnses: [{ address: '119.29.29.29', family: 1 }],
        linkAddresses: [{ address: { address: '10.77.0.5', family: 1 } }] } } });
  stepAbility['networkNetId'] = 101;
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 21, dup_fd: 22,
    generation: 3, net_id: 101 });
  G['__NB_CD_POLL_ANSWER_QUEUE'] = [
    JSON.stringify({ ok: true, step: 'pre', phase: '', id: 'cd-C-M-1', kind: 'M', column: 'C',
      fd: -1, dup_fd: -1, remote: 'unknown', marker: 'cd-C-M-1-pre', result: 'pre',
      reason: 'send-pre-now', line: '' }),
    JSON.stringify({ ok: true, step: 'receipt', phase: 'receipt',
      id: 'cd-C-M-1', kind: 'M', column: 'C', fd: 21, dup_fd: 22, remote: '192.0.2.9:443',
      marker: 'cd-C-M-1-post', result: 'receipt', reason: 'ok', line: '' })
  ];
  resetLog();
  (stepAbility['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netAvailable', 202, 202);
  await drainProbe();
  await drainProbe();
  const cdPreLast = (G['__NB_CD_PRE_LAST'] ?? {}) as Record<string, unknown>;
  check('N22 the switch-instant step poll sends the PRE control on the new network first',
    controlCalls() === 2 && cdPreLast['id'] === 'cd-C-M-1' && cdPreLast['result'] === 'hit' &&
    JSON.stringify(G['__NB_CONTROL_IDS']) === JSON.stringify(['cd-C-M-1-pre', 'cd-C-M-1-post']) &&
    lastLine('N2_CD_SANDWICH_SHELL|').length > 0,
    JSON.stringify({ ids: G['__NB_CONTROL_IDS'], pre: cdPreLast }));
  const cdPostLast = (G['__NB_CD_POST_LAST'] ?? {}) as Record<string, unknown>;
  check('N22 a receipt step sends the POST control with the core\'s marker and reports the verdict',
    controlCalls() === 2 &&
    JSON.stringify(G['__NB_CONTROL_LAST']) === JSON.stringify({ armed: N2_EVIDENCE_OPTIONS,
      target: '119.29.29.119:443', protocol: 'tcp', id: 'cd-C-M-1-post' }) &&
    cdPostLast['id'] === 'cd-C-M-1' && cdPostLast['result'] === 'observed' &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=post|marker=cd-C-M-1-post|result=observed|') > 0,
    JSON.stringify({ calls: controlCalls(), control: G['__NB_CONTROL_LAST'], post: cdPostLast,
      line: lastLine('N2_CD_SANDWICH_SHELL|') }));

  // N22b: a `closed` step is a terminal line the CORE already emitted (the
  // receipt timeout and the Wb exemption arrive this way); the shell mirrors the
  // core's phase and result and only clears its latch.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  seedNetCtx({ __NB_NET_DEFAULT_ID: 101,
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [], bearerTypes: [1] } } });
  G['__NB_CD_PLAN_ANSWER'] = JSON.stringify({ ok: true, result: 'planned', reason: 'ok',
    id: 'cd-C-M-1', pre_marker: 'cd-C-M-1-pre', post_marker: 'cd-C-M-1-post', line: '' });
  const timeoutAbility = armedAbility();
  resetLog();
  (timeoutAbility['pollN2CdPlan'] as () => void)();
  await drainProbe();
  await drainProbe();
  G['__NB_CD_POLL_ANSWER'] = JSON.stringify({ ok: true, step: 'closed', phase: 'receipt',
    id: 'cd-C-M-1', kind: 'M', column: 'C', fd: -1, dup_fd: -1, remote: 'unknown',
    marker: '', result: 'unsat', reason: 'receipt-wait-exceeded', line: '' });
  resetLog();
  (timeoutAbility['pollN2CdStep'] as () => void)();
  check('N22b a closed step mirrors the core\'s phase and result (a receipt timeout is UNSAT)',
    lastLine('N2_CD_SANDWICH_SHELL|') === 'N2_CD_SANDWICH_SHELL|requestId=' + REQUEST_ID +
    '|id=cd-C-M-1|column=C|kind=M|rep=1|phase=receipt|marker=none|result=unsat|' +
    'reason=receipt-wait-exceeded|diag=true|is_evidence=false',
    lastLine('N2_CD_SANDWICH_SHELL|'));

  // N23: the shell's ring half. An armed session forwards its own line families
  // verbatim; an unarmed one forwards nothing (the ring does not exist before
  // the arm and is stopped by every arming change).
  resetFs();
  resetRequestState();
  const ringAbility = armedAbility();
  (ringAbility['n2RingAppend'] as (l: string) => void)('N2_FIXTURE|line=1');
  check('N23 an armed session forwards a shell line to the ring verbatim',
    JSON.stringify(G['__NB_RING']) === JSON.stringify(['N2_FIXTURE|line=1']),
    JSON.stringify(G['__NB_RING']));
  const unarmedRing = freshAbility(mod, FIXTURE_FILESDIR);
  (unarmedRing['n2RingAppend'] as (l: string) => void)('N2_FIXTURE|line=2');
  check('N23 an unarmed session forwards NOTHING (the ring is armed-only)',
    JSON.stringify(G['__NB_RING']) === JSON.stringify(['N2_FIXTURE|line=1']),
    JSON.stringify(G['__NB_RING']));
  check('N23 every arming change stops the ring and the teardown reports its counters',
    (() => {
      resetFs();
      resetRequestState();
      setArmFile(false, undefined);
      const disarmAbility = freshAbility(mod, FIXTURE_FILESDIR);
      disarmAbility['context'] = { filesDir: FIXTURE_FILESDIR };
      resetLog();
      (disarmAbility['readDiagSwitches'] as () => void)();
      return Number(G['__NB_RING_DISABLED'] ?? 0) === 1;
    })(),
    JSON.stringify(G['__NB_RING_DISABLED']));

  // N24: the ring FETCH fallback (`n2_ring_dump`), now SEQ-addressed. The shell
  // validates the document, asks the CORE for the manifest first, forwards the
  // document VERBATIM, re-emits exactly what the core answered (chunk records +
  // one summary, both to HiLog only), ADVANCES NOTHING (the receipt mirrors the
  // core's watermark as `ack_watermark=` and no ACK call is made), and writes
  // only its own `N2_RING_DUMP_SHELL` receipt back into the ring.
  resetFs();
  resetRequestState();
  const RING_PAGE = JSON.stringify({ ok: true, result: 'dumped', reason: 'ok', from_seq: 0,
    to_seq: 9, count: 2, emitted: 2, frozen_tail_seq: 12, first_seq: 1, last_seq: 12,
    records: 12, bytes: 512, malformed: 0, lost_records: 0, overflow: false, acked_seq: 4,
    lines: ['N2_RING_CHUNK|seq=7|of=2|line=N2_A|1', 'N2_RING_CHUNK|seq=9|of=2|line=N2_B|2'],
    summary: 'N2_RING_DUMP|from_seq=0|to_seq=9|count=2|emitted=2|frozen_tail_seq=12|' +
      'first_seq=1|last_seq=12|records=12|bytes=512|malformed=0|lost_records=0|overflow=false|' +
      'acked_seq=4|result=dumped|reason=ok|diag=true|is_evidence=false' });
  seedFile(RING_DUMP_PATH, RING_DUMP_PAYLOAD);
  G['__NB_RING_DUMP_ANSWER'] = RING_PAGE;
  const dumpAbility = freshAbility(mod, FIXTURE_FILESDIR);
  dumpAbility['requestId'] = REQUEST_ID;
  resetLog();
  (dumpAbility['pollN2RingDump'] as () => void)();
  check('N24 the dump re-emits the core\'s chunk records verbatim, in durable seq order',
    JSON.stringify(linesWith('N2_RING_CHUNK|')) === JSON.stringify([
      'N2_RING_CHUNK|seq=7|of=2|line=N2_A|1',
      'N2_RING_CHUNK|seq=9|of=2|line=N2_B|2'
    ]),
    JSON.stringify(linesWith('N2_RING_CHUNK|')));
  check('N24 the manifest is asked for FIRST and the document is forwarded VERBATIM, then consumed',
    Number(G['__NB_RING_MANIFEST_CALLS'] ?? 0) === 1 &&
    JSON.stringify(G['__NB_RING_DUMP_LAST']) === JSON.stringify({ request: RING_DUMP_PAYLOAD }) &&
    lastLine('N2_RING_DUMP|').indexOf('N2_RING_DUMP|from_seq=0|to_seq=9|') === 0 &&
    fileOf(RING_DUMP_PATH) === undefined,
    JSON.stringify({ manifest: G['__NB_RING_MANIFEST_CALLS'], dump: G['__NB_RING_DUMP_LAST'],
      line: lastLine('N2_RING_DUMP|'), file: fileOf(RING_DUMP_PATH) }));
  check('N24 the dump NEVER advances the ACK: it reports the watermark and calls no ACK',
    Number(G['__NB_RING_ACK_CALLS'] ?? 0) === 0 &&
    lastLine('N2_RING_DUMP_SHELL|') === 'N2_RING_DUMP_SHELL|requestId=' + REQUEST_ID +
    '|from_seq=0|to_seq=9|count=2|emitted=2|ack_watermark=4|result=dumped|reason=ok|' +
    'diag=true|is_evidence=false',
    JSON.stringify({ ack: G['__NB_RING_ACK_CALLS'], ackLast: G['__NB_RING_ACK_LAST'],
      line: lastLine('N2_RING_DUMP_SHELL|') }));
  check('N24 while armed ONLY the shell receipt enters the ring (never a chunk, never the summary)',
    (() => {
      resetFs();
      resetRequestState();
      seedFile(RING_DUMP_PATH, RING_DUMP_PAYLOAD);
      G['__NB_RING_DUMP_ANSWER'] = RING_PAGE;
      const armedDump = armedAbility();
      resetLog();
      (armedDump['pollN2RingDump'] as () => void)();
      const ring = (G['__NB_RING'] as string[] ?? []);
      return ring.length === 1 &&
        ring[0] === 'N2_RING_DUMP_SHELL|requestId=' + REQUEST_ID +
          '|from_seq=0|to_seq=9|count=2|emitted=2|ack_watermark=4|result=dumped|reason=ok|' +
          'diag=true|is_evidence=false' &&
        logs().join('\n').indexOf('N2_RING_CHUNK|') >= 0;
    })(),
    JSON.stringify(G['__NB_RING']));
  check('N24 a legacy line-index document is refused by name, consumed and never reaches the core',
    (() => {
      resetFs();
      resetRequestState();
      seedFile(RING_DUMP_PATH, '{"start":0,"count":2}');
      resetLog();
      (dumpAbility['pollN2RingDump'] as () => void)();
      return Number(G['__NB_RING_MANIFEST_CALLS'] ?? 0) === 0 &&
        Number(G['__NB_RING_DUMP_CALLS'] ?? 0) === 0 &&
        Number(G['__NB_RING_ACK_CALLS'] ?? 0) === 0 &&
        lastLine('N2_RING_DUMP_SHELL|') === 'N2_RING_DUMP_SHELL|requestId=' + REQUEST_ID +
        '|from_seq=0|to_seq=0|count=0|emitted=0|ack_watermark=-1|result=rejected|' +
        'reason=legacy-start-index-unsupported|diag=true|is_evidence=false' &&
        linesWith('N2_RING_CHUNK|').length === 0 && fileOf(RING_DUMP_PATH) === undefined;
    })(),
    lastLine('N2_RING_DUMP_SHELL|'));

  // N25: the EXPLICIT host ACK channel (`n2_ring_ack`) — the ONLY path that may
  // advance the retrieved watermark. The host writes the document through the
  // private channel after it has persisted the fetched bytes; the shell parses
  // it (both keys required), forwards it VERBATIM and mirrors the core's answer
  // (`advanced=`). A refusal / a throw reports `advanced=false` and can never
  // read as a confirmation.
  resetFs();
  resetRequestState();
  const RING_ACK_OK = JSON.stringify({ ok: true, result: 'acked', reason: 'ok',
    session: 'auto-1-2-3', up_to_seq: 9, acked_seq: 9, previous_acked_seq: 0, advanced: true });
  seedFile(RING_ACK_PATH, RING_ACK_PAYLOAD);
  G['__NB_RING_ACK_ANSWER'] = RING_ACK_OK;
  const ackAbility = freshAbility(mod, FIXTURE_FILESDIR);
  ackAbility['requestId'] = REQUEST_ID;
  resetLog();
  (ackAbility['pollN2RingAck'] as () => void)();
  check('N25 the ACK document is forwarded VERBATIM and the request consumed exactly once',
    JSON.stringify(G['__NB_RING_ACK_LAST']) === JSON.stringify({ request: RING_ACK_PAYLOAD }) &&
    fileOf(RING_ACK_PATH) === undefined,
    JSON.stringify({ ack: G['__NB_RING_ACK_LAST'], file: fileOf(RING_ACK_PATH) }));
  check('N25 the accepted ACK receipt names the arm, the watermark and the advancement',
    lastLine('N2_RING_ACK_SHELL|') === 'N2_RING_ACK_SHELL|requestId=' + REQUEST_ID +
    '|sess=auto-1-2-3|up_to_seq=9|acked_seq=9|advanced=true|result=acked|reason=ok|' +
    'diag=true|is_evidence=false',
    lastLine('N2_RING_ACK_SHELL|'));
  check('N25 while armed the ACK receipt enters the ring (it is an ACK event)',
    (() => {
      resetFs();
      resetRequestState();
      seedFile(RING_ACK_PATH, RING_ACK_PAYLOAD);
      G['__NB_RING_ACK_ANSWER'] = RING_ACK_OK;
      const armedAck = armedAbility();
      resetLog();
      (armedAck['pollN2RingAck'] as () => void)();
      const ring = (G['__NB_RING'] as string[] ?? []);
      return ring.length === 1 && ring[0] === 'N2_RING_ACK_SHELL|requestId=' + REQUEST_ID +
        '|sess=auto-1-2-3|up_to_seq=9|acked_seq=9|advanced=true|result=acked|reason=ok|' +
        'diag=true|is_evidence=false';
    })(),
    JSON.stringify(G['__NB_RING']));
  check('N25 a REFUSED core answer reports no advancement and the untouched watermark',
    (() => {
      resetFs();
      resetRequestState();
      seedFile(RING_ACK_PATH, '{"session":"arm-dead","up_to_seq":9}');
      G['__NB_RING_ACK_ANSWER'] = JSON.stringify({ ok: false, result: 'rejected',
        reason: 'session-mismatch', session: 'arm-dead', up_to_seq: 9, acked_seq: 0,
        previous_acked_seq: 0, advanced: false });
      resetLog();
      (ackAbility['pollN2RingAck'] as () => void)();
      return lastLine('N2_RING_ACK_SHELL|') === 'N2_RING_ACK_SHELL|requestId=' + REQUEST_ID +
        '|sess=arm-dead|up_to_seq=9|acked_seq=0|advanced=false|result=rejected|' +
        'reason=session-mismatch|diag=true|is_evidence=false' &&
        fileOf(RING_ACK_PATH) === undefined;
    })(),
    lastLine('N2_RING_ACK_SHELL|'));
  check('N25 a malformed / session-less document is refused by name and never reaches the core',
    (() => {
      const cases: string[] = ['{"up_to_seq":9}', '{"session":""}', '', 'not json',
        '{"session":"a"}', '{"session":"a","up_to_seq":-1}'];
      const expected: string[] = ['invalid-session', 'invalid-session', 'empty-request',
        'invalid-json', 'invalid-request', 'invalid-request'];
      for (let i = 0; i < cases.length; i++) {
        resetFs();
        resetRequestState();
        seedFile(RING_ACK_PATH, cases[i]);
        resetLog();
        (ackAbility['pollN2RingAck'] as () => void)();
        const line = lastLine('N2_RING_ACK_SHELL|');
        const refused = line === 'N2_RING_ACK_SHELL|requestId=' + REQUEST_ID +
          '|sess=-|up_to_seq=-1|acked_seq=-1|advanced=false|result=rejected|reason=' +
          expected[i] + '|diag=true|is_evidence=false';
        if (!refused || Number(G['__NB_RING_ACK_CALLS'] ?? 0) !== 0 ||
          fileOf(RING_ACK_PATH) !== undefined) {
          return false;
        }
      }
      return true;
    })(),
    JSON.stringify({ calls: G['__NB_RING_ACK_CALLS'],
      line: lastLine('N2_RING_ACK_SHELL|') }));
  check('N25 a core THROW is reported as rejected with no watermark and consumes the request',
    (() => {
      resetFs();
      resetRequestState();
      seedFile(RING_ACK_PATH, RING_ACK_PAYLOAD);
      const throwing = freshAbility(mod, FIXTURE_FILESDIR);
      throwing['requestId'] = REQUEST_ID;
      const original = G['__NB_CORE_ACK_THROW'];
      G['__NB_CORE_ACK_THROW'] = true;
      resetLog();
      try {
        (throwing['pollN2RingAck'] as () => void)();
      } finally {
        G['__NB_CORE_ACK_THROW'] = original;
      }
      return lastLine('N2_RING_ACK_SHELL|') === 'N2_RING_ACK_SHELL|requestId=' + REQUEST_ID +
        '|sess=auto-1-2-3|up_to_seq=9|acked_seq=-1|advanced=false|result=rejected|' +
        'reason=core-throw|diag=true|is_evidence=false' &&
        fileOf(RING_ACK_PATH) === undefined;
    })(),
    lastLine('N2_RING_ACK_SHELL|'));
  check('N25 an untouched ACK channel is a silent no-op (no file, no call, no line)',
    (() => {
      resetFs();
      resetRequestState();
      resetLog();
      (ackAbility['pollN2RingAck'] as () => void)();
      return linesWith('N2_RING_ACK_SHELL|').length === 0 &&
        Number(G['__NB_RING_ACK_CALLS'] ?? 0) === 0;
    })(),
    JSON.stringify(linesWith('N2_RING_ACK_SHELL|')));

  // ---------------- C8 (2026-09-18): peer_overlay_ip verbatim bridging -----
  // The parallel posctl-target fix adds the optional non-secret
  // `peer_overlay_ip` to the arm file options: the ACTUAL n2h-peer overlay
  // address (multi-peer maps no longer mean "first()"). The shell NEVER
  // validates or repairs it — present ⇒ forwarded verbatim into the native
  // opts JSON, absent ⇒ the key stays absent (no default injected), wrong-type
  // ⇒ forwarded as that value and the CORE's strict bare-IPv4 + peer-map check
  // refuses it. No marker field and no log literal changes.
  const overlayAbility = freshAbility(mod, FIXTURE_FILESDIR);
  overlayAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"peer_overlay_ip":"10.99.0.7"}');
  resetLog();
  tickArm(overlayAbility, WG_READY);
  const overlayOpts = JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C8 a present peer_overlay_ip reaches the native opts JSON verbatim',
    overlayOpts['peer_overlay_ip'] === '10.99.0.7' &&
    String(G['__NB_ARM_OPTS']).indexOf('"peer_overlay_ip":"10.99.0.7"') > 0,
    String(G['__NB_ARM_OPTS']));
  check('C8 the existing arm fields ride unchanged next to the new key',
    overlayOpts['posctl_port'] === 40001 && overlayOpts['probe_wait_ms'] === 800,
    String(G['__NB_ARM_OPTS']));

  const noOverlayAbility = freshAbility(mod, FIXTURE_FILESDIR);
  noOverlayAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"posctl_port":41000}');
  resetLog();
  tickArm(noOverlayAbility, WG_READY);
  const noOverlayOpts =
    JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C8 an absent peer_overlay_ip stays absent (no key, no default injected)',
    noOverlayOpts['peer_overlay_ip'] === undefined &&
    String(G['__NB_ARM_OPTS']).indexOf('peer_overlay_ip') < 0,
    String(G['__NB_ARM_OPTS']));

  const badOverlayAbility = freshAbility(mod, FIXTURE_FILESDIR);
  badOverlayAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"peer_overlay_ip":123}');
  resetLog();
  tickArm(badOverlayAbility, WG_READY);
  const badOverlayOpts =
    JSON.parse(String(G['__NB_ARM_OPTS'] ?? '{}')) as Record<string, unknown>;
  check('C8 a wrong-type value is forwarded AS THAT VALUE (never dropped, never defaulted)',
    badOverlayOpts['peer_overlay_ip'] === 123 &&
    String(G['__NB_ARM_OPTS']).indexOf('"peer_overlay_ip":123') > 0,
    String(G['__NB_ARM_OPTS']));

  // ---------------- C9 (2026-09-20): dns_nxdomain_echo verbatim bridging ----
  // v4 effective preregistration §3 path① / §6.2 item 2: the optional
  // non-secret `dns_nxdomain_echo` enable for the device collector's dns
  // probe. The shell is a PURE FORWARDER — present (true OR false) ⇒ the key
  // lands in the native opts JSON verbatim (existing fields unchanged),
  // absent ⇒ the key stays ABSENT (no `false` default injected; the core's
  // documented OFF is what an omitted key means), wrong-type (JSON string /
  // number) ⇒ forwarded AS THAT VALUE for the core's strict boolean check to
  // refuse. No marker field and no log literal changes.
  const echoOnAbility = freshAbility(mod, FIXTURE_FILESDIR);
  echoOnAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":true}');
  resetLog();
  tickArm(echoOnAbility, WG_READY);
  const echoOnRaw = String(G['__NB_ARM_OPTS']);
  const echoOnOpts = JSON.parse(echoOnRaw) as Record<string, unknown>;
  check('C9 a present dns_nxdomain_echo=true reaches the native opts JSON verbatim',
    echoOnOpts['dns_nxdomain_echo'] === true &&
    echoOnRaw.indexOf('"dns_nxdomain_echo":true') > 0,
    echoOnRaw);
  check('C9 the existing arm fields ride unchanged next to dns_nxdomain_echo',
    echoOnOpts['posctl_port'] === 40001 && echoOnOpts['probe_wait_ms'] === 800,
    echoOnRaw);

  const echoOffAbility = freshAbility(mod, FIXTURE_FILESDIR);
  echoOffAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":false}');
  resetLog();
  tickArm(echoOffAbility, WG_READY);
  const echoOffRaw = String(G['__NB_ARM_OPTS']);
  const echoOffOpts = JSON.parse(echoOffRaw) as Record<string, unknown>;
  check('C9 an explicit false is forwarded as false (never dropped as "absent")',
    echoOffOpts['dns_nxdomain_echo'] === false &&
    echoOffRaw.indexOf('"dns_nxdomain_echo":false') > 0,
    echoOffRaw);

  const noEchoAbility = freshAbility(mod, FIXTURE_FILESDIR);
  noEchoAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"posctl_port":41000}');
  resetLog();
  tickArm(noEchoAbility, WG_READY);
  const noEchoRaw = String(G['__NB_ARM_OPTS']);
  const noEchoOpts = JSON.parse(noEchoRaw) as Record<string, unknown>;
  check('C9 an absent dns_nxdomain_echo stays absent (no key, no false injected)',
    noEchoOpts['dns_nxdomain_echo'] === undefined &&
    noEchoRaw.indexOf('dns_nxdomain_echo') < 0,
    noEchoRaw);

  const badEchoAbility = freshAbility(mod, FIXTURE_FILESDIR);
  badEchoAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":"true"}');
  resetLog();
  tickArm(badEchoAbility, WG_READY);
  const badEchoRaw = String(G['__NB_ARM_OPTS']);
  const badEchoOpts = JSON.parse(badEchoRaw) as Record<string, unknown>;
  check('C9 a wrong-type string is forwarded AS THAT VALUE (never coerced, never dropped)',
    badEchoOpts['dns_nxdomain_echo'] === 'true' &&
    badEchoRaw.indexOf('"dns_nxdomain_echo":"true"') > 0,
    badEchoRaw);

  const numEchoAbility = freshAbility(mod, FIXTURE_FILESDIR);
  numEchoAbility['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, '{"dns_nxdomain_echo":1}');
  resetLog();
  tickArm(numEchoAbility, WG_READY);
  const numEchoRaw = String(G['__NB_ARM_OPTS']);
  const numEchoOpts = JSON.parse(numEchoRaw) as Record<string, unknown>;
  check('C9 a wrong-type number is forwarded AS THAT VALUE (never coerced, never dropped)',
    numEchoOpts['dns_nxdomain_echo'] === 1 &&
    numEchoRaw.indexOf('"dns_nxdomain_echo":1') > 0,
    numEchoRaw);

  // ---------------- P (2026-09-18): the iface-bytes availability probe -----
  // The OFFICIAL per-NIC byte API (@ohos.net.statistics getIfaceRxBytes /
  // getIfaceTxBytes, since 10, NO permission on the iface methods) is the
  // availability question run 0017 left open: the app cannot read
  // /proc/net/dev (Permission denied, counters=[]). The probe is wired ONLY
  // into the arm SUCCESS branch — these checks pin exactly that: when it
  // fires, what it records (values / codes / begin-end / negative control),
  // what it must never do (fire without a native ARM success, turn an error
  // into 0, print a bogus value, run twice per instance), and the unchanged
  // literals around the insertion point. Bytes only: nothing here feeds
  // Evidence.counters, no packet count exists, no ARM reconciliation runs.
  await drainProbe(); // settle the probes the earlier groups' ARMED ticks armed
  resetLog();

  // P1: the REAL arm success branch triggers exactly ONE probe round.
  const probeAbility = freshAbility(mod, FIXTURE_FILESDIR);
  probeAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 1234, 'n2h-none0': 0 },
    { 'vpn-tun': 5678, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  const callsAtP1 = statsCalls().length;
  tickArm(probeAbility, WG_READY);
  await drainProbe();
  const p1Calls = statsCalls().slice(callsAtP1);
  check('P1 the arm SUCCESS branch triggers exactly ONE probe round: the four documented calls',
    linesWith(PROBE_PREFIX).length === 2 &&
    p1Calls.length === 4 &&
    p1Calls.indexOf('getIfaceRxBytes:vpn-tun') >= 0 &&
    p1Calls.indexOf('getIfaceTxBytes:vpn-tun') >= 0 &&
    p1Calls.indexOf('getIfaceRxBytes:n2h-none0') >= 0 &&
    p1Calls.indexOf('getIfaceTxBytes:n2h-none0') >= 0,
    JSON.stringify({ probeLines: linesWith(PROBE_PREFIX), calls: p1Calls }));

  // P2: the vpn-tun line — exact head (values + none-errors), digit-shaped
  // begin/end with finished >= started, the diag flags as the tail.
  const p1Vpn = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'vpn-tun');
  const p1VpnLine = p1Vpn.length === 1 ? p1Vpn[0] : '';
  const p1Started = fieldOf(p1VpnLine, 'startedUnixMs');
  const p1Finished = fieldOf(p1VpnLine, 'finishedUnixMs');
  check('P2 vpn-tun line: resolved values + none-errors + digit begin/end + diag flags',
    p1VpnLine === 'N2H_IFACE_BYTES_PROBE|requestId=' + REQUEST_ID +
    '|interface=vpn-tun|rxBytes=1234|txBytes=5678|rxError=none|txError=none|' +
    'startedUnixMs=' + p1Started + '|finishedUnixMs=' + p1Finished +
    '|diag=true|is_evidence=false' &&
    /^\d+$/.test(p1Started) && /^\d+$/.test(p1Finished) &&
    Number(p1Finished) >= Number(p1Started),
    p1VpnLine);

  // P3: the negative control line — the 0-answer of the nonexistent name is
  // recorded verbatim (a finding about the unknown-name shape, never read as
  // a proof that the vpn-tun value works).
  const p1Neg = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'n2h-none0');
  check('P3 the fixed nonexistent name IS sampled and its 0-answer recorded verbatim',
    p1Neg.length === 1 &&
    p1Neg[0] === 'N2H_IFACE_BYTES_PROBE|requestId=' + REQUEST_ID +
    '|interface=n2h-none0|rxBytes=0|txBytes=0|rxError=none|txError=none|' +
    'startedUnixMs=' + fieldOf(p1Neg[0], 'startedUnixMs') +
    '|finishedUnixMs=' + fieldOf(p1Neg[0], 'finishedUnixMs') +
    '|diag=true|is_evidence=false',
    p1Neg.join('\n'));

  // P4: per-direction honesty — a rejected call leaves its byte field
  // unavailable with the raw code kept; the OTHER direction still records;
  // the round still made its full four calls.
  const errAbility = freshAbility(mod, FIXTURE_FILESDIR);
  errAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': { code: 2103012 }, 'n2h-none0': 7 },
    { 'vpn-tun': 99, 'n2h-none0': { code: 2100002 } });
  setArmFile(true, undefined);
  const callsAtP4 = statsCalls().length;
  resetLog();
  tickArm(errAbility, WG_READY);
  await drainProbe();
  const p4Vpn = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'vpn-tun');
  const p4Neg = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'n2h-none0');
  check('P4 rx rejects -> rxBytes=unavailable with the code kept, tx still records',
    p4Vpn.length === 1 &&
    fieldOf(p4Vpn[0], 'rxBytes') === 'unavailable' &&
    fieldOf(p4Vpn[0], 'rxError') === '2103012' &&
    fieldOf(p4Vpn[0], 'txBytes') === '99' &&
    fieldOf(p4Vpn[0], 'txError') === 'none',
    p4Vpn.join('\n'));
  check('P4 tx rejects -> txBytes=unavailable with the code kept, rx still records',
    p4Neg.length === 1 &&
    fieldOf(p4Neg[0], 'rxBytes') === '7' &&
    fieldOf(p4Neg[0], 'rxError') === 'none' &&
    fieldOf(p4Neg[0], 'txBytes') === 'unavailable' &&
    fieldOf(p4Neg[0], 'txError') === '2100002',
    p4Neg.join('\n'));
  check('P4 the failed round still made its full four statistics calls',
    statsCalls().length - callsAtP4 === 4, String(statsCalls().length - callsAtP4));

  // P5: NO native ARM success ⇒ NO probe. Three shapes: the plain-start
  // default (no ticket), the ticket with the session gate still closed, and
  // the gate open but the core answering not-ok.
  const callsAtP5 = statsCalls().length;
  const noTicketAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(false, undefined);
  resetLog();
  tickArm(noTicketAbility, WG_READY);
  await drainProbe();
  check('P5a no arm ticket (the plain-start default) -> the ONE skip line, no probe call',
    linesWith('N2H_COLLECTOR_ARM_SKIPPED|').length === 1 &&
    linesWith(PROBE_PREFIX).length === 0 &&
    statsCalls().length === callsAtP5, JSON.stringify(logs()));

  const waitingAbility = freshAbility(mod, FIXTURE_FILESDIR);
  setArmFile(true, undefined, '');
  resetLog();
  tickArm(waitingAbility, { wg: { device_up: true, ready: false } });
  await drainProbe();
  check('P5b ticket present but the session gate closed -> one WAITING line, no probe call',
    linesWith('N2H_COLLECTOR_WAITING|').length === 1 &&
    linesWith(PROBE_PREFIX).length === 0 &&
    statsCalls().length === callsAtP5, JSON.stringify(logs()));

  const refusedAbility = freshAbility(mod, FIXTURE_FILESDIR);
  refusedAbility['n2hAcceptedExclusionGeneration'] = 0;
  G['__NB_ARM_ANSWER'] = '{"ok":false,"error":"forced-by-p5"}';
  setArmFile(true, undefined);
  resetLog();
  tickArm(refusedAbility, WG_READY);
  G['__NB_ARM_ANSWER'] = undefined;
  await drainProbe();
  check('P5c the core answers not-ok -> one ARM_FAILED line, no probe call (out.ok gates it)',
    linesWith('N2H_COLLECTOR_ARM_FAILED|').length === 1 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 0 &&
    linesWith(PROBE_PREFIX).length === 0 &&
    statsCalls().length === callsAtP5, JSON.stringify(logs()));

  // P6: the OPEN probe inherits the arm's one-shot semantics; the FIRST
  // watcher TRIGGER after the arm carries exactly the WINDOW close edge (the
  // W group pins its shape — here only that it is the one extra thing, then
  // silence): no new PROBE line, exactly one close round (two statistics
  // calls), and the REAL tick after that repeats nothing at all.
  // roam-1005-0014: the production per-tick vpn-tun PRESENCE probe adds ONE
  // getIfaceRxBytes call per live-status tick (never a WINDOW/PROBE line, and
  // never a tx call — the close round stays provable by its single
  // getIfaceTxBytes), so the expected counts carry that +1.
  G['__NB_STATUS'] = {
    running: true, state: 'connected', peer_count: 1, route_count: 2,
    reconnects: 0, terminal: false, wg: { device_up: true, ready: true }
  };
  const p6Tick = watcherTick(probeAbility);
  const callsAtP6 = statsCalls().length;
  resetLog();
  p6Tick();
  await drainProbe();
  const p6CloseCalls = statsCalls().length - callsAtP6;
  check('P6 first post-arm tick: no PROBE repeat, exactly the one WINDOW close round',
    linesWith(PROBE_PREFIX).length === 0 &&
    linesWith(WINDOW_PREFIX).length === 1 &&
    fieldOf(linesWith(WINDOW_PREFIX)[0], 'phase') === 'close' &&
    p6CloseCalls === 3,
    JSON.stringify({ window: linesWith(WINDOW_PREFIX), calls: p6CloseCalls }));
  resetLog();
  p6Tick();
  await drainProbe();
  check('P6 the tick after the close edge repeats nothing: no line, no statistics call',
    statsCalls().length === callsAtP6 + p6CloseCalls + 1 &&
    linesWith(WINDOW_PREFIX).length === 0 && linesWith(PROBE_PREFIX).length === 0,
    JSON.stringify({ window: linesWith(WINDOW_PREFIX), probe: linesWith(PROBE_PREFIX) }));

  // P7: value honesty — a RESOLVED value that is not a finite nonnegative
  // byte count is never printed and never turned into 0 (token
  // invalid-value); a resolved 0 IS finite nonnegative and prints as 0.
  const honestAbility = freshAbility(mod, FIXTURE_FILESDIR);
  honestAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': -5, 'n2h-none0': 0 },
    { 'vpn-tun': NaN, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  resetLog();
  tickArm(honestAbility, WG_READY);
  await drainProbe();
  const p7Lines = linesWith(PROBE_PREFIX);
  check('P7 negative/NaN answers -> unavailable + invalid-value (no 0-laundering)',
    p7Lines.length === 2 &&
    fieldOf(p7Lines[0], 'interface') === 'vpn-tun' &&
    fieldOf(p7Lines[0], 'rxBytes') === 'unavailable' &&
    fieldOf(p7Lines[0], 'rxError') === 'invalid-value' &&
    fieldOf(p7Lines[0], 'txBytes') === 'unavailable' &&
    fieldOf(p7Lines[0], 'txError') === 'invalid-value',
    JSON.stringify(p7Lines));
  check('P7 the negative control still records its honest zeros in the same round',
    p7Lines.length === 2 &&
    fieldOf(p7Lines[1], 'interface') === 'n2h-none0' &&
    fieldOf(p7Lines[1], 'rxBytes') === '0' &&
    fieldOf(p7Lines[1], 'txBytes') === '0' &&
    fieldOf(p7Lines[1], 'rxError') === 'none',
    JSON.stringify(p7Lines));

  // P8: wiring pins on the byte-identical sources — the call site sits INSIDE
  // the arm success branch (ok -> ARMED literal -> probe -> else), the probe
  // module carries the fixed names and stays byte-only (no /proc read, no
  // getUid*/permission surface, no message text), and module.json5 grew no
  // permission.
  const probeSource: string =
    readFileSync(join(dir, 'NetBirdInterfaceStatsProbe.ts')).toString();
  const okBranchAt = extSource.indexOf('if (answer.ok === true) {');
  const armedLit = extSource.indexOf(
    "'N2H_COLLECTOR_ARMED|requestId=%{public}s|gen=%{public}d|openedMono=%{public}d|'");
  const probeCall = extSource.indexOf('runInterfaceStatsProbe(this.requestId, (): void => {');
  const elseAt = extSource.indexOf('} else {', armedLit);
  check('P8 the probe call sits INSIDE the arm success branch (ok -> ARMED literal -> probe -> else)',
    okBranchAt > 0 && armedLit > okBranchAt && probeCall > armedLit && elseAt > probeCall &&
    extSource.indexOf(
      "import { runInterfaceStatsProbe, runNetworkContextProbe, runInterfaceStatsWindowClose,\n  readInterfaceStatsBurst } from\n  './NetBirdInterfaceStatsProbe';") > 0,
    JSON.stringify({ okBranchAt, armedLit, probeCall, elseAt }));
  check('P8 the probe module pins the two fixed names, the marker, and the byte-only scope',
    probeSource.indexOf("const PROBE_IFACE: string = 'vpn-tun';") > 0 &&
    probeSource.indexOf("const PROBE_NEGATIVE_IFACE: string = 'n2h-none0';") > 0 &&
    probeSource.indexOf(PROBE_PREFIX) > 0 &&
    probeSource.indexOf('diag=true|is_evidence=false') > 0 &&
    probeSource.indexOf('@ohos.file.fs') < 0 &&
    probeSource.indexOf('readTextSync') < 0 &&
    probeSource.indexOf('getUidRxBytes') < 0 &&
    probeSource.indexOf('.message') < 0,
    'probe module pin');
  check('P8 the aborted-sample guard prints codes/names only (never an exception message)',
    extSource.indexOf(
      "'N2H_IFACE_BYTES_PROBE_ABORTED|requestId=%{public}s|code=%{public}s|'") > 0 &&
    extSource.indexOf("'name=%{public}s|diag=true|is_evidence=false',") > 0,
    'aborted-guard pin');
  check('P8 no permission surface was added (module.json5 carries no GET_NETWORK_STATS)',
    readFileSync(join(HERE, '..', 'src', 'main', 'module.json5')).toString()
      .indexOf('GET_NETWORK_STATS') < 0,
    'module.json5 pin');

  // ---------------- W (2026-09-20): the iface-bytes WINDOW two-edge read ----
  // v4 preregistration §4 (ii), C7 same-source-calibration prerequisite: ONE
  // OPEN/CLOSE pair on the SAME official per-NIC byte API. The OPEN edge
  // re-renders the arm branch's own vpn-tun sample (SAME numbers — no second
  // read); the CLOSE edge is ONE fresh vpn-tun read issued on the first
  // watcher TRIGGER after that open line — at the very top of the tick,
  // before every early return (the 2026-09-20 fix), never the arming tick,
  // never twice, never without the open latch. The window groups below replay
  // REAL watcher ticks (the captured-interval mechanism), not the arm hook
  // alone. Diagnostic-only: the readings are C7 counting input, never
  // Evidence.counters/notes, never the judge. The same field discipline as
  // the probe line holds (codes only, never a message).
  // The full live-session document the REAL tick polls (the B-group shape).
  const W_HEALTHY: Record<string, unknown> = {
    running: true, state: 'connected', peer_count: 1, route_count: 2,
    reconnects: 0, terminal: false, wg: { device_up: true, ready: true }
  };
  const openAbility = freshAbility(mod, FIXTURE_FILESDIR);
  openAbility['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 2100, 'n2h-none0': 0 }, { 'vpn-tun': 3100, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  const callsAtW1 = statsCalls().length;
  resetLog();
  tickArm(openAbility, WG_READY);
  await drainProbe();
  const w1Lines = linesWith(WINDOW_PREFIX);
  const w1Open = w1Lines.length === 1 ? w1Lines[0] : '';
  const w1ProbeVpn = linesWith(PROBE_PREFIX).filter((l: string): boolean =>
    fieldOf(l, 'interface') === 'vpn-tun');
  check('W1 the arming tick emits exactly the open edge, no close, and only the four probe calls',
    w1Lines.length === 1 &&
    fieldOf(w1Open, 'phase') === 'open' &&
    fieldOf(w1Open, 'requestId') === REQUEST_ID &&
    fieldOf(w1Open, 'pairRequestId') === REQUEST_ID &&
    fieldOf(w1Open, 'interface') === 'vpn-tun' &&
    fieldOf(w1Open, 'rxBytes') === '2100' && fieldOf(w1Open, 'txBytes') === '3100' &&
    fieldOf(w1Open, 'rxError') === 'none' && fieldOf(w1Open, 'txError') === 'none' &&
    /^\d+$/.test(fieldOf(w1Open, 'startedUnixMs')) &&
    /^\d+$/.test(fieldOf(w1Open, 'finishedUnixMs')) &&
    fieldOf(w1Open, 'diag') === 'true' && fieldOf(w1Open, 'is_evidence') === 'false' &&
    statsCalls().length - callsAtW1 === 4,
    JSON.stringify({ window: w1Lines, calls: statsCalls().slice(callsAtW1) }));
  check('W1 the open edge is the SAME sample as the PROBE line (no second read: identical numbers)',
    w1ProbeVpn.length === 1 &&
    fieldOf(w1ProbeVpn[0], 'startedUnixMs') === fieldOf(w1Open, 'startedUnixMs') &&
    fieldOf(w1ProbeVpn[0], 'finishedUnixMs') === fieldOf(w1Open, 'finishedUnixMs') &&
    fieldOf(w1ProbeVpn[0], 'rxBytes') === fieldOf(w1Open, 'rxBytes') &&
    fieldOf(w1ProbeVpn[0], 'txBytes') === fieldOf(w1Open, 'txBytes'),
    JSON.stringify({ probe: w1ProbeVpn, open: w1Open }));

  // W2: the CLOSE edge lands on the NEXT watcher TRIGGER — a REAL replayed
  // tick, same requestId/pair, FRESH values, exactly two vpn-tun calls,
  // close.started >= open.finished.
  G['__NB_STATUS'] = W_HEALTHY;
  const w2Tick = watcherTick(openAbility);
  const callsAtW2 = statsCalls().length;
  resetLog();
  seedStats({ 'vpn-tun': 2400, 'n2h-none0': 0 }, { 'vpn-tun': 3600, 'n2h-none0': 0 });
  w2Tick();
  await drainProbe();
  const w2Lines = linesWith(WINDOW_PREFIX);
  const w2Close = w2Lines.length === 1 ? w2Lines[0] : '';
  const w2Calls = statsCalls().slice(callsAtW2);
  check('W2 the NEXT tick emits exactly the close edge: same requestId/pair, fresh values, two vpn-tun calls',
    w2Lines.length === 1 &&
    fieldOf(w2Close, 'phase') === 'close' &&
    fieldOf(w2Close, 'requestId') === REQUEST_ID &&
    fieldOf(w2Close, 'pairRequestId') === REQUEST_ID &&
    fieldOf(w2Close, 'interface') === 'vpn-tun' &&
    fieldOf(w2Close, 'rxBytes') === '2400' && fieldOf(w2Close, 'txBytes') === '3600' &&
    fieldOf(w2Close, 'rxError') === 'none' && fieldOf(w2Close, 'txError') === 'none' &&
    fieldOf(w2Close, 'diag') === 'true' && fieldOf(w2Close, 'is_evidence') === 'false' &&
    // roam-1005-0014: 3 = close rx+tx PLUS the production presence probe's
    // one rx (this tick's status was live); the close round itself stays
    // pinned by the single getIfaceTxBytes call.
    w2Calls.length === 3 &&
    w2Calls.indexOf('getIfaceRxBytes:vpn-tun') >= 0 &&
    w2Calls.indexOf('getIfaceTxBytes:vpn-tun') >= 0,
    JSON.stringify({ window: w2Lines, calls: w2Calls }));
  check('W2 the close edge is not earlier than the open edge (started >= open.finished)',
    Number(fieldOf(w2Close, 'startedUnixMs')) >= Number(fieldOf(w1Open, 'finishedUnixMs')),
    JSON.stringify({ openFinished: fieldOf(w1Open, 'finishedUnixMs'),
      closeStarted: fieldOf(w2Close, 'startedUnixMs') }));

  // W3: one-shot — the ticks after the close edge repeat nothing (no new
  // WINDOW/PROBE line; the per-tick status lines are the pre-existing watcher
  // chatter, not window output). roam-1005-0014: the only statistics traffic
  // left is the production presence probe's ONE rx per live-status tick (the
  // two replayed ticks below), never a WINDOW/PROBE line nor a tx call.
  const callsAtW3 = statsCalls().length;
  resetLog();
  w2Tick();
  w2Tick();
  await drainProbe();
  check('W3 ticks after the close edge repeat nothing: no WINDOW/PROBE line, no statistics call',
    linesWith(WINDOW_PREFIX).length === 0 && linesWith(PROBE_PREFIX).length === 0 &&
    statsCalls().length === callsAtW3 + 2,
    JSON.stringify({ window: linesWith(WINDOW_PREFIX), probe: linesWith(PROBE_PREFIX) }));

  // W4: a REJECTED close read prints unavailable + that direction's raw code,
  // never throws, never launders 0 — and the diagnostic flow continues (the
  // one-shot latch was consumed at issue; the next REAL tick repeats nothing).
  const w4Ability = freshAbility(mod, FIXTURE_FILESDIR);
  w4Ability['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 10, 'n2h-none0': 0 }, { 'vpn-tun': 20, 'n2h-none0': 0 });
  setArmFile(true, undefined);
  resetLog();
  tickArm(w4Ability, WG_READY);
  await drainProbe();
  const w4Open = linesWith(WINDOW_PREFIX);
  G['__NB_STATUS'] = W_HEALTHY;
  const w4Tick = watcherTick(w4Ability);
  seedStats({ 'vpn-tun': { code: 2103012 } }, { 'vpn-tun': { code: 2100002 } });
  const callsAtW4 = statsCalls().length;
  resetLog();
  w4Tick();
  await drainProbe();
  const w4Lines = linesWith(WINDOW_PREFIX);
  const w4Close = w4Lines.length === 1 ? w4Lines[0] : '';
  check('W4 a rejected close read: unavailable + the raw code per direction, no 0-laundering, no throw',
    w4Open.length === 1 && fieldOf(w4Open[0], 'phase') === 'open' &&
    w4Lines.length === 1 && fieldOf(w4Close, 'phase') === 'close' &&
    fieldOf(w4Close, 'requestId') === REQUEST_ID &&
    fieldOf(w4Close, 'pairRequestId') === REQUEST_ID &&
    fieldOf(w4Close, 'rxBytes') === 'unavailable' &&
    fieldOf(w4Close, 'rxError') === '2103012' &&
    fieldOf(w4Close, 'txBytes') === 'unavailable' &&
    fieldOf(w4Close, 'txError') === '2100002' &&
    linesWith('N2H_IFACE_BYTES_WINDOW_CLOSE_ABORTED|').length === 0,
    JSON.stringify({ open: w4Open, close: w4Lines }));
  resetLog();
  w4Tick();
  await drainProbe();
  check('W4 the diagnostic flow continues: the next tick repeats nothing (one-shot consumed at issue)',
    linesWith(WINDOW_PREFIX).length === 0 && linesWith(PROBE_PREFIX).length === 0 &&
    // roam-1005-0014: 4 = the close round's rx+tx (this snapshot predates the
    // close tick) PLUS one presence-probe rx on each of the two live ticks.
    statsCalls().length === callsAtW4 + 4,
    JSON.stringify({ window: linesWith(WINDOW_PREFIX), calls: statsCalls().slice(callsAtW4) }));

  // W5: NO native ARM success ⇒ zero close reads. Three shapes, mirroring P5,
  // each driven through REAL watcher ticks: the close hook runs on EVERY tick
  // and must stay silent without the open edge.
  const w5Cases: Array<{ name: string; arm: () => void; status: Record<string, unknown> }> = [
    { name: 'W5a no arm ticket (the plain-start default)', status: W_HEALTHY,
      arm: (): void => { setArmFile(false, undefined); } },
    { name: 'W5b ticket present but the session gate closed',
      status: { ...W_HEALTHY, wg: { device_up: true, ready: false } },
      arm: (): void => { setArmFile(true, undefined, ''); } },
    { name: 'W5c the core answers not-ok', status: W_HEALTHY,
      arm: (): void => {
        G['__NB_ARM_ANSWER'] = '{"ok":false,"error":"forced-by-w5"}';
        setArmFile(true, undefined);
      } }
  ];
  for (const w5Case of w5Cases) {
    const w5Ability = freshAbility(mod, FIXTURE_FILESDIR);
    w5Ability['n2hAcceptedExclusionGeneration'] = 0;
    w5Case.arm();
    G['__NB_STATUS'] = w5Case.status;
    const w5Tick = watcherTick(w5Ability);
    const callsAtW5 = statsCalls().length;
    resetLog();
    w5Tick();
    await drainProbe();
    w5Tick();
    await drainProbe();
    G['__NB_ARM_ANSWER'] = undefined;
    check(w5Case.name + ' -> zero WINDOW lines, zero close read',
      linesWith(WINDOW_PREFIX).length === 0 &&
      linesWith(PROBE_PREFIX).length === 0 &&
      // roam-1005-0014: the two live-status ticks carry only the production
      // presence probe's one rx each — no WINDOW/PROBE line, no close read.
      statsCalls().length === callsAtW5 + 2, JSON.stringify(linesWith(WINDOW_PREFIX)));
  }

  // W7 (2026-09-20 fix): the close edge is NOT bypassed by the tick forms that
  // early-return before the arm hook. v4 §4/§6.2 mechanical two-edge
  // requirement: once the open edge happened, the NEXT watcher trigger issues
  // the close edge EXACTLY once — including a TERMINAL tick, a
  // recreate-pending tick, and a status-unreachable tick. REAL ticks only.
  const w7Forms: Array<{ name: string; status: Record<string, unknown> | null }> = [
    { name: 'W7a terminal tick', status: { ...W_HEALTHY, terminal: true, state: 'failed' } },
    { name: 'W7b recreate-pending tick',
      status: { ...W_HEALTHY, recreate: { required: true, reason: 'forced-by-w7' } } },
    { name: 'W7c status-unreachable tick', status: null }
  ];
  for (const w7Form of w7Forms) {
    const w7Ability = freshAbility(mod, FIXTURE_FILESDIR);
    w7Ability['n2hAcceptedExclusionGeneration'] = 0;
    seedStats({ 'vpn-tun': 5100, 'n2h-none0': 0 }, { 'vpn-tun': 6100, 'n2h-none0': 0 });
    setArmFile(true, undefined);
    G['__NB_STATUS'] = W_HEALTHY;
    const w7Tick = watcherTick(w7Ability);
    // the ARMING tick (healthy form): the open edge only, never a close.
    const callsAtW7Open = statsCalls().length;
    resetLog();
    w7Tick();
    await drainProbe();
    const w7Armed = linesWith(WINDOW_PREFIX);
    const w7OpenCalls = statsCalls().length - callsAtW7Open;
    // the FORM tick: the close edge EXACTLY once — issued at the very top of
    // the tick, before the terminal/recreate/poll-failure early return.
    seedStats({ 'vpn-tun': 5400, 'n2h-none0': 0 }, { 'vpn-tun': 6600, 'n2h-none0': 0 });
    G['__NB_STATUS'] = w7Form.status;
    const callsAtW7Close = statsCalls().length;
    resetLog();
    w7Tick();
    await drainProbe();
    const w7CloseLines = linesWith(WINDOW_PREFIX);
    const w7Close = w7CloseLines.length === 1 ? w7CloseLines[0] : '';
    const w7Calls = statsCalls().slice(callsAtW7Close);
    // the tick after: silent (the one-shot latch was consumed at issue).
    resetLog();
    G['__NB_STATUS'] = W_HEALTHY;
    w7Tick();
    await drainProbe();
    check(w7Form.name + ': the open edge only on the arming tick',
      w7Armed.length === 1 && fieldOf(w7Armed[0], 'phase') === 'open' &&
      // roam-1005-0014: 5 = the open round's four calls PLUS the arming
      // tick's one presence-probe rx (its status was live for every form).
      w7OpenCalls === 5,
      JSON.stringify({ window: w7Armed,
        calls: statsCalls().slice(callsAtW7Open, callsAtW7Open + w7OpenCalls) }));
    check(w7Form.name + ' still issues the close edge EXACTLY once (top of the tick, before the early return)',
      w7CloseLines.length === 1 && fieldOf(w7Close, 'phase') === 'close' &&
      fieldOf(w7Close, 'requestId') === REQUEST_ID &&
      fieldOf(w7Close, 'pairRequestId') === REQUEST_ID &&
      // roam-1005-0014: a live-status form tick adds one presence-probe rx to
      // the close round's rx+tx; the status-UNREACHABLE form (W7c) never
      // reaches the status branch, so it adds none.
      w7Calls.length === (w7Form.status === null ? 2 : 3) &&
      w7Calls.indexOf('getIfaceRxBytes:vpn-tun') >= 0 &&
      w7Calls.indexOf('getIfaceTxBytes:vpn-tun') >= 0,
      JSON.stringify({ window: w7CloseLines, calls: w7Calls }));
    check(w7Form.name + ' leaves no residue: the tick after repeats nothing',
      linesWith(WINDOW_PREFIX).length === 0 && linesWith(PROBE_PREFIX).length === 0,
      JSON.stringify({ window: linesWith(WINDOW_PREFIX), probe: linesWith(PROBE_PREFIX) }));
  }

  // W6: source pins on the byte-identical sources — the window path adds no
  // timer and no loop, the close read is fire-and-forget behind the open
  // latch, its hook rides the watcher tick at its VERY TOP (before the status
  // read and the terminal/recreate early returns, no longer inside the arm
  // hook), and the latch callback writes one boolean inside the ok-branch.
  const armHookAt = extSource.indexOf('private maybeArmN2hCollector(');
  const armEarlyReturnAt = extSource.indexOf(
    'if (this.n2hArmAttempted || this.destroyRequested) {', armHookAt);
  const armHookHead = armEarlyReturnAt > armHookAt
    ? extSource.slice(armHookAt, armEarlyReturnAt) : '';
  // Since 2026-09-26 the extension carries a SECOND `setInterval((): void => {`
  // (the default-OFF S5 iface-bytes burst, in the arm ok-branch ABOVE the
  // watcher), so this anchor must say WHICH timer it means: the watcher's own,
  // searched from the watcher's signature.
  const watcherStartAt = extSource.indexOf('private startStatusAndNetcfgWatcher(): void {');
  const setIntervalAt = extSource.indexOf('setInterval((): void => {', watcherStartAt);
  const closeHookAt = extSource.indexOf('this.maybeCloseInterfaceStatsWindow();');
  const statusReadAt = extSource.indexOf(
    'const status: ConnectorStatusResult | null = pollStatusOrNull();');
  const terminalReturnAt = extSource.indexOf('if (status.terminal === true) {');
  // The window path only — the extension keeps its PRE-EXISTING watcher
  // interval, so the pin is scoped to the close latch method and the two
  // probe-module window functions, not to the whole extension.
  const w6CloseFnMatch = /private maybeCloseInterfaceStatsWindow\(\): void \{[\s\S]*?\n  \}/
    .exec(extSource);
  const w6CloseFn = w6CloseFnMatch === null ? '' : w6CloseFnMatch[0];
  const w6EmitFnMatch = /function emitWindowLine\([\s\S]*?\n\}/.exec(probeSource);
  const w6EmitFn = w6EmitFnMatch === null ? '' : w6EmitFnMatch[0];
  const w6CloseExportMatch = /export async function runInterfaceStatsWindowClose\([\s\S]*?\n\}/
    .exec(probeSource);
  const w6CloseExport = w6CloseExportMatch === null ? '' : w6CloseExportMatch[0];
  const w6NoTimerLoop = (text: string): boolean =>
    !/setInterval|setTimeout|while \(|for \(/.test(text);
  check('W6 the window path adds no timer and no loop (fire-and-forget, no await added)',
    w6NoTimerLoop(w6CloseFn) && w6NoTimerLoop(w6EmitFn) && w6NoTimerLoop(w6CloseExport) &&
    probeSource.indexOf('setInterval') < 0 && probeSource.indexOf('setTimeout') < 0 &&
    extSource.indexOf('await this.maybeCloseInterfaceStatsWindow') < 0 &&
    extSource.indexOf('await runInterfaceStatsWindowClose') < 0,
    JSON.stringify({ closeFn: w6CloseFn.length, emitFn: w6EmitFn.length,
      closeExport: w6CloseExport.length }));
  check('W6 the close hook rides the watcher tick at its VERY TOP, before the status read and every early return',
    watcherStartAt > 0 && setIntervalAt > watcherStartAt && closeHookAt > setIntervalAt &&
    closeHookAt < statusReadAt &&
    statusReadAt > closeHookAt && closeHookAt < terminalReturnAt &&
    terminalReturnAt > statusReadAt &&
    extSource.split('this.maybeCloseInterfaceStatsWindow();').length === 2,
    JSON.stringify({ watcherStartAt, setIntervalAt, closeHookAt, statusReadAt, terminalReturnAt,
      callSites: extSource.split('this.maybeCloseInterfaceStatsWindow();').length - 1 }));
  check('W6 the arm hook no longer carries the close hook (the early-return path cannot gate the close edge)',
    armHookAt > 0 && armEarlyReturnAt > armHookAt &&
    armHookHead.indexOf('maybeCloseInterfaceStatsWindow') < 0,
    JSON.stringify({ armHookAt, armEarlyReturnAt, headLen: armHookHead.length }));
  const w6GuardAt = extSource.indexOf(
    'if (!this.n2hIfaceWindowOpenSeen || this.n2hIfaceWindowCloseDone ||');
  const w6IssueAt = extSource.indexOf('runInterfaceStatsWindowClose(this.requestId).catch(');
  check('W6 the close read issues only behind the open latch, once (consumed at issue), with a last-resort catch',
    w6GuardAt > 0 && extSource.indexOf('this.n2hIfaceWindowCloseDone = true;') > w6GuardAt &&
    w6IssueAt > w6GuardAt && probeCall > 0,
    JSON.stringify({ w6GuardAt, w6IssueAt }));
  const latchCbAt = extSource.indexOf('this.n2hIfaceWindowOpenSeen = true;');
  check('W6 the open latch is one boolean written inside the ok-branch after the probe call',
    okBranchAt > 0 && armedLit > okBranchAt && probeCall > armedLit &&
    latchCbAt > probeCall && elseAt > latchCbAt, JSON.stringify({ okBranchAt, probeCall, latchCbAt, elseAt }));
  // "The read happens AFTER the native arm": the open edge sits in the
  // ok-branch past the `n2h_collector_arm(...)` call, and the close edge is
  // gated by the open latch that only that same branch sets (so the close read
  // cannot precede a successful native arm however the hook is scheduled).
  const armCallAt = extSource.indexOf('core.n2h_collector_arm(');
  check('W6 both window reads issue only AFTER the native arm call (open in the ok-branch, close behind the open latch)',
    armCallAt > 0 && probeCall > armCallAt && latchCbAt > armCallAt &&
    w6GuardAt > 0 && w6IssueAt > w6GuardAt,
    JSON.stringify({ armCallAt, probeCall, latchCbAt }));
  check('W6 both window edges are emitted by the byte-only probe module, diag-only',
    probeSource.indexOf(WINDOW_PREFIX) > 0 &&
    probeSource.indexOf("emitWindowLine(requestId, 'open', PROBE_IFACE, vpnSample)") > 0 &&
    probeSource.indexOf("emitWindowLine(requestId, 'close', PROBE_IFACE, sample)") > 0 &&
    probeSource.indexOf("'diag=true|is_evidence=false'") > 0,
    'window source pin');

  // ---------------- IB (2026-09-26): the S5 iface-bytes BURST --------------
  // The 500 ms survey series behind the arm ticket's DEFAULT-OFF
  // `diag_iface_bytes_burst` switch (n2h-kit/c7-same-source-plan-20260926.md
  // §3 mechanism a / §6.1): while it runs, every tick is ONE vpn-tun sample on
  // the SAME official per-NIC byte API the window edges use, emitted on its own
  // `N2H_IFACE_BYTES_BURST|` line. The timer belongs to the EXTENSION (it must
  // be cancellable from the teardown paths), so the tests
  // below CAPTURE it — the watcherTick mechanism, no real timer is ever
  // scheduled — and drive its handler by hand; clearInterval is captured too,
  // so "the handle is really cleared" is an assertion about the REAL call site.
  // The load-bearing half is the default: IB1/IB7 pin zero timers and zero
  // BURST lines for an absent, a `false`, and a wrong-typed key, and IB2 pins
  // that a burst tick emits no PROBE/WINDOW line.
  const burstTimers: Array<{ id: number; handler: () => void; ms: number }> = [];
  const burstCleared: number[] = [];
  const previousSetInterval = G['setInterval'];
  const previousClearInterval = G['clearInterval'];
  function installBurstTimerSpy(): void {
    burstTimers.length = 0;
    burstCleared.length = 0;
    G['setInterval'] = (handler: () => void, ms?: number): number => {
      const id: number = 9100 + burstTimers.length;
      burstTimers.push({ id: id, handler: handler, ms: ms ?? -1 });
      return id;
    };
    G['clearInterval'] = (id: number): void => { burstCleared.push(id); };
  }
  function restoreBurstTimerSpy(): void {
    G['setInterval'] = previousSetInterval;
    if (previousClearInterval === undefined) {
      delete G['clearInterval'];
    } else {
      G['clearInterval'] = previousClearInterval;
    }
  }
  // The handler of the ONE captured burst interval (a no-op if none was
  // scheduled, so a failed start surfaces as an assertion, never a TypeError).
  function burstHandler(): () => void {
    return burstTimers.length === 0
      ? (): void => { /* nothing was scheduled */ }
      : burstTimers[burstTimers.length - 1].handler;
  }
  // Drive ONE captured burst tick and let its async sample land.
  async function burstTickOnce(): Promise<void> {
    burstHandler()();
    await drainProbe();
  }
  // The 500 ms cadence and the tick cap are read OFF the production source, so
  // the assertions below follow the shipped numbers instead of restating them.
  const burstMsMatch = /const N2H_IFACE_BURST_INTERVAL_MS: number = (\d+);/.exec(extSource);
  const burstCapMatch = /const N2H_IFACE_BURST_MAX_TICKS: number = (\d+);/.exec(extSource);
  const burstMs = burstMsMatch === null ? -1 : Number(burstMsMatch[1]);
  const burstCap = burstCapMatch === null ? -1 : Number(burstCapMatch[1]);
  // The same JSON-shape equality the D group's deepEq uses, declared HERE so
  // this group never depends on a declaration that comes later in the file.
  function ibEq(a: unknown, b: unknown): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
  }

  // IB1: the default. A ticket WITHOUT the key (the normal case: the launcher
  // never sends it) reads OFF and starts nothing — zero new lines, zero
  // intervals, and the handle stays -1.
  installBurstTimerSpy();
  const ibOff = freshAbility(mod, FIXTURE_FILESDIR);
  ibOff['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 1000, 'n2h-none0': 0 }, { 'vpn-tun': 2000, 'n2h-none0': 0 });
  setArmFile(true, undefined, JSON.stringify({ posctl_port: 41000 }));
  (ibOff['readDiagSwitches'] as () => void)();
  resetLog();
  tickArm(ibOff, WG_READY);
  await drainProbe();
  check('IB1 the burst key ABSENT (default): the arm still ARMS, but no BURST line and ' +
    'no interval is created, and the handle stays -1',
    linesWith('N2H_COLLECTOR_ARMED|').length === 1 &&
    linesWith(BURST_PREFIX).length === 0 &&
    burstTimers.length === 0 && ibOff['diagIfaceBytesBurst'] === false &&
    ibOff['n2hIfaceBurstTimer'] === -1,
    JSON.stringify({ burst: linesWith(BURST_PREFIX), timers: burstTimers.length,
      field: ibOff['diagIfaceBytesBurst'], handle: ibOff['n2hIfaceBurstTimer'] }));

  // IB2: the switch ON (literal JSON true) — the arm success branch starts
  // EXACTLY ONE bounded 500 ms interval, and one tick = one BURST line with
  // seq starting at 0, the fixed field order, the seeded values, digit
  // begin/end and the diag tail. The negative-control name is NOT read (two
  // calls per tick, both vpn-tun).
  installBurstTimerSpy();
  const ibOn = freshAbility(mod, FIXTURE_FILESDIR);
  ibOn['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 1000, 'n2h-none0': 7 }, { 'vpn-tun': 2000, 'n2h-none0': 7 });
  setArmFile(true, undefined,
    JSON.stringify({ diag_iface_bytes_burst: true, posctl_port: 41000 }));
  (ibOn['readDiagSwitches'] as () => void)();
  const ibCallsAtArm = statsCalls().length;
  resetLog();
  tickArm(ibOn, WG_READY);
  await drainProbe();
  const ibOptsRaw = String(G['__NB_ARM_OPTS'] ?? '');
  check('IB2 the switch ON: the arm ARMS, starts exactly ONE 500 ms interval, and the ' +
    'burst key never reaches the core arm options',
    ibOn['diagIfaceBytesBurst'] === true &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 1 &&
    burstTimers.length === 1 && burstTimers[0].ms === burstMs && burstMs === 500 &&
    burstCleared.length === 0 && ibOn['n2hIfaceBurstTimer'] === burstTimers[0].id &&
    statsCalls().length - ibCallsAtArm === 4 &&
    ibOptsRaw.length > 0 && ibOptsRaw.indexOf('diag_iface_bytes_burst') < 0,
    JSON.stringify({ ms: burstTimers.length === 0 ? -1 : burstTimers[0].ms, cap: burstCap,
      handle: ibOn['n2hIfaceBurstTimer'], opts: ibOptsRaw }));
  const ibCallsAtTick1 = statsCalls().length;
  // The arm tick itself emits the open WINDOW edge and the two PROBE lines
  // (IB2's arm above); what must stay flat is the BURST TICK's own output.
  const ibWindowBeforeTick = linesWith(WINDOW_PREFIX).length;
  const ibProbeBeforeTick = linesWith(PROBE_PREFIX).length;
  await burstTickOnce();
  const ib1Lines = linesWith(BURST_PREFIX);
  const ib1Line = ib1Lines.length === 1 ? ib1Lines[0] : '';
  const ib1Calls = statsCalls().slice(ibCallsAtTick1);
  check('IB2 one tick = ONE BURST line: seq=0, fixed field order, seeded bytes, digit ' +
    'begin/end, diag tail — and exactly the two vpn-tun calls (no negative control)',
    ib1Lines.length === 1 &&
    ib1Line === 'N2H_IFACE_BYTES_BURST|requestId=' + REQUEST_ID +
    '|seq=0|interface=vpn-tun|rxBytes=1000|txBytes=2000|rxError=none|txError=none|' +
    'startedUnixMs=' + fieldOf(ib1Line, 'startedUnixMs') +
    '|finishedUnixMs=' + fieldOf(ib1Line, 'finishedUnixMs') +
    '|diag=true|is_evidence=false' &&
    /^\d+$/.test(fieldOf(ib1Line, 'startedUnixMs')) &&
    /^\d+$/.test(fieldOf(ib1Line, 'finishedUnixMs')) &&
    Number(fieldOf(ib1Line, 'finishedUnixMs')) >= Number(fieldOf(ib1Line, 'startedUnixMs')) &&
    fieldOf(ib1Line, 'diag') === 'true' && fieldOf(ib1Line, 'is_evidence') === 'false' &&
    ib1Calls.length === 2 &&
    ib1Calls.indexOf('getIfaceRxBytes:vpn-tun') >= 0 &&
    ib1Calls.indexOf('getIfaceTxBytes:vpn-tun') >= 0,
    JSON.stringify({ line: ib1Line, calls: ib1Calls }));
  check('IB2 the burst TICK emits no WINDOW and no PROBE line (the W/P line-count ' +
    'assertions stay true)',
    linesWith(WINDOW_PREFIX).length === ibWindowBeforeTick &&
    linesWith(PROBE_PREFIX).length === ibProbeBeforeTick,
    JSON.stringify({ window: linesWith(WINDOW_PREFIX).length,
      probe: linesWith(PROBE_PREFIX).length,
      before: [ibWindowBeforeTick, ibProbeBeforeTick] }));
  await burstTickOnce();
  check('IB2 seq increments per tick (0 then 1) and the interval is still armed',
    linesWith(BURST_PREFIX).length === 2 && fieldOf(lastLine(BURST_PREFIX), 'seq') === '1' &&
    burstCleared.length === 0 && ibOn['n2hIfaceBurstTimer'] === burstTimers[0].id,
    JSON.stringify({ seqs: linesWith(BURST_PREFIX).map((l: string): string => fieldOf(l, 'seq')),
      cleared: burstCleared }));

  // IB3: the hard cap. The series stops itself after burstCap ticks (cap
  // = 40 at 500 ms = 20 s: the C7 same-source pairing needs two sample points
  // ≥10 s apart, and the app-side window close edge — ~5 s after ARMED — is no
  // longer a stop condition) — the tick past the cap clears the interval and
  // emits NOTHING, and a callback that somehow
  // survived the cancellation is inert (the -1 handle is checked first). The
  // log is NOT reset here: the assertion below is about the WHOLE series, so
  // the two IB2 ticks are part of it.
  let ibGuard = 0;
  while (burstCleared.length === 0 && ibGuard < burstCap + 5) {
    ibGuard++;
    await burstTickOnce();
  }
  const ibCapLines = linesWith(BURST_PREFIX);
  const ibCapSeqs = ibCapLines.map((l: string): string => fieldOf(l, 'seq'));
  const ibOptsAfterCap = ibOn['n2hIfaceBurstTimer'];
  check('IB3 the cap: the series emits tick 0 through cap-1 and stops (' + String(burstCap) +
    ' lines, seq strictly increasing, one clearInterval of the armed handle)',
    burstCap === 40 && ibCapLines.length === burstCap &&
    ibEq(ibCapSeqs, Array.from({ length: burstCap },
      (v: unknown, i: number): string => String(i))) &&
    ibEq(burstCleared, [burstTimers[0].id]) && ibOptsAfterCap === -1,
    JSON.stringify({ cap: burstCap, lines: ibCapLines.length, seqs: ibCapSeqs,
      cleared: burstCleared, handle: ibOptsAfterCap }));
  resetLog();
  await burstTickOnce();
  // The whole series emitted exactly burstCap lines, each with TWO vpn-tun
  // reads: the arm round's four calls plus burstCap ticks at two calls each.
  const ibExpectedCalls = ibCallsAtArm + 4 + burstCap * 2;
  check('IB3 after the cap: a further (contradictory) callback emits nothing and clears ' +
    'nothing twice',
    linesWith(BURST_PREFIX).length === 0 && statsCalls().length === ibExpectedCalls &&
    ibEq(burstCleared, [burstTimers[0].id]),
    JSON.stringify({ burst: linesWith(BURST_PREFIX), cleared: burstCleared,
      calls: statsCalls().length, expected: ibExpectedCalls }));

  // IB4: the WINDOW CLOSE edge must NOT stop it. The close edge is issued on
  // the first watcher TRIGGER after the open edge (W2) — the app-side boundary
  // of the collector WINDOW (the native N2H_COLLECTOR_CLOSE line is emitted by
  // the core and is not observable in this shell). The C7 same-source
  // calibration pairs two sample points ≥10 s apart and depends only on the
  // peer's steady one-frame-per-second traffic, NOT on the app window being
  // open — and this edge lands only ~5 s after ARMED — so a real replayed tick
  // must leave the burst interval ARMED, the series must keep emitting past it,
  // and only the tick cap may clear the handle.
  installBurstTimerSpy();
  const ibClose = freshAbility(mod, FIXTURE_FILESDIR);
  ibClose['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 1000, 'n2h-none0': 0 }, { 'vpn-tun': 2000, 'n2h-none0': 0 });
  setArmFile(true, undefined, JSON.stringify({ diag_iface_bytes_burst: true }));
  (ibClose['readDiagSwitches'] as () => void)();
  resetLog();
  tickArm(ibClose, WG_READY);
  await drainProbe();
  await burstTickOnce();
  const ibCloseLinesBefore = linesWith(BURST_PREFIX).length;
  G['__NB_STATUS'] = W_HEALTHY;
  const ibCloseTick = watcherTick(ibClose);
  resetLog();
  ibCloseTick();
  await drainProbe();
  const ibCloseWindow = linesWith(WINDOW_PREFIX);
  check('IB4 the window close edge fires exactly once and leaves the burst interval ' +
    'ARMED (arming tick + one survey tick before it)',
    ibCloseLinesBefore === 1 && burstTimers.length === 1 &&
    ibCloseWindow.length === 1 && fieldOf(ibCloseWindow[0], 'phase') === 'close' &&
    burstCleared.length === 0 && ibClose['n2hIfaceBurstTimer'] === burstTimers[0].id,
    JSON.stringify({ before: ibCloseLinesBefore, window: ibCloseWindow,
      cleared: burstCleared, handle: ibClose['n2hIfaceBurstTimer'] }));
  await burstTickOnce();
  check('IB4 after the close edge the series still emits: the next tick is seq=1 and the ' +
    'armed handle is unchanged',
    linesWith(BURST_PREFIX).length === 1 &&
    fieldOf(lastLine(BURST_PREFIX), 'seq') === '1' &&
    burstCleared.length === 0 && ibClose['n2hIfaceBurstTimer'] === burstTimers[0].id,
    JSON.stringify({ burst: linesWith(BURST_PREFIX), cleared: burstCleared,
      handle: ibClose['n2hIfaceBurstTimer'] }));
  // Drive the series (which survived the close edge) on to its cap: it must
  // emit seq 1..cap-1 — the pre-close seq=0 tick had its log reset away above —
  // and only THEN clear the interval, exactly once.
  let ibCloseGuard = 0;
  while (burstCleared.length === 0 && ibCloseGuard < burstCap + 5) {
    ibCloseGuard++;
    await burstTickOnce();
  }
  const ibCloseCapLines = linesWith(BURST_PREFIX);
  check('IB4 past the close edge the series runs to the cap and only then clears the ' +
    'interval (' + String(burstCap - 1) + ' post-close lines, seq 1..cap-1, one ' +
    'clearInterval of the armed handle)',
    ibCloseCapLines.length === burstCap - 1 &&
    ibEq(ibCloseCapLines.map((l: string): string => fieldOf(l, 'seq')),
      Array.from({ length: burstCap - 1 }, (v: unknown, i: number): string => String(i + 1))) &&
    ibEq(burstCleared, [burstTimers[0].id]) && ibClose['n2hIfaceBurstTimer'] === -1,
    JSON.stringify({ lines: ibCloseCapLines.length, cap: burstCap,
      seqs: ibCloseCapLines.map((l: string): string => fieldOf(l, 'seq')),
      cleared: burstCleared, handle: ibClose['n2hIfaceBurstTimer'] }));

  // IB5: the TEARDOWN path (stopStatusAndNetcfgWatcher — connector death,
  // fail-closed recreate teardown, onDestroy) stops it IMMEDIATELY: no waiting
  // for the cap, and independently of the window close edge.
  installBurstTimerSpy();
  const ibStop = freshAbility(mod, FIXTURE_FILESDIR);
  ibStop['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 1000, 'n2h-none0': 0 }, { 'vpn-tun': 2000, 'n2h-none0': 0 });
  setArmFile(true, undefined, JSON.stringify({ diag_iface_bytes_burst: true }));
  (ibStop['readDiagSwitches'] as () => void)();
  resetLog();
  tickArm(ibStop, WG_READY);
  await drainProbe();
  await burstTickOnce();
  (ibStop['stopStatusAndNetcfgWatcher'] as () => void)();
  resetLog();
  await burstTickOnce();
  check('IB5 stopStatusAndNetcfgWatcher stops the burst: the armed handle is cleared, the ' +
    'field is -1 and the next callback emits nothing',
    burstTimers.length === 1 && ibEq(burstCleared, [burstTimers[0].id]) &&
    ibStop['n2hIfaceBurstTimer'] === -1 && linesWith(BURST_PREFIX).length === 0,
    JSON.stringify({ cleared: burstCleared, handle: ibStop['n2hIfaceBurstTimer'],
      burst: linesWith(BURST_PREFIX) }));

  // IB6: onDestroy (the real lifecycle call) clears it as well.
  installBurstTimerSpy();
  const ibDestroy = freshAbility(mod, FIXTURE_FILESDIR);
  ibDestroy['n2hAcceptedExclusionGeneration'] = 0;
  seedStats({ 'vpn-tun': 1000, 'n2h-none0': 0 }, { 'vpn-tun': 2000, 'n2h-none0': 0 });
  setArmFile(true, undefined, JSON.stringify({ diag_iface_bytes_burst: true }));
  (ibDestroy['readDiagSwitches'] as () => void)();
  resetLog();
  tickArm(ibDestroy, WG_READY);
  await drainProbe();
  await burstTickOnce();
  (ibDestroy['onDestroy'] as () => void)();
  resetLog();
  await burstTickOnce();
  check('IB6 onDestroy stops the burst: the armed handle is cleared, the field is -1 and ' +
    'the next callback emits nothing',
    burstTimers.length === 1 && ibEq(burstCleared, [burstTimers[0].id]) &&
    ibDestroy['n2hIfaceBurstTimer'] === -1 && ibDestroy['destroyRequested'] === true &&
    linesWith(BURST_PREFIX).length === 0,
    JSON.stringify({ cleared: burstCleared, handle: ibDestroy['n2hIfaceBurstTimer'],
      burst: linesWith(BURST_PREFIX) }));

  // IB7: the type discipline of the SAME readDiagSwitch every other switch
  // uses. A string / a number / a JSON null is OFF, prints the ONE ignored-key
  // line naming THIS key, and starts nothing; a literal `false` is a valid
  // typed OFF and stays SILENT.
  const ibTyped: Array<{ name: string; value: unknown; ignored: boolean }> = [
    { name: 'a STRING "true"', value: 'true', ignored: true },
    { name: 'a NUMBER 1', value: 1, ignored: true },
    { name: 'a JSON null', value: null, ignored: true },
    { name: 'a literal false', value: false, ignored: false }
  ];
  for (const ibCase of ibTyped) {
    installBurstTimerSpy();
    const ibAbility = freshAbility(mod, FIXTURE_FILESDIR);
    ibAbility['n2hAcceptedExclusionGeneration'] = 0;
    seedStats({ 'vpn-tun': 1000, 'n2h-none0': 0 }, { 'vpn-tun': 2000, 'n2h-none0': 0 });
    setArmFile(true, undefined, JSON.stringify({ diag_iface_bytes_burst: ibCase.value }));
    resetLog();
    (ibAbility['readDiagSwitches'] as () => void)();
    const ibIgnored = linesWith('N2H_DIAG_OPTION_IGNORED|');
    tickArm(ibAbility, WG_READY);
    await drainProbe();
    check('IB7 ' + ibCase.name + ': the burst is OFF, nothing is started, and the ignored ' +
      'line ' + (ibCase.ignored ? 'IS' : 'is NOT') + ' printed for this key',
      ibAbility['diagIfaceBytesBurst'] === false && burstTimers.length === 0 &&
      linesWith(BURST_PREFIX).length === 0 &&
      (ibCase.ignored
        ? ibEq(ibIgnored, [
          'N2H_DIAG_OPTION_IGNORED|key=diag_iface_bytes_burst|reason=not-boolean|' +
          'diag=true|is_evidence=false'])
        : ibIgnored.length === 0),
      JSON.stringify({ ignored: ibIgnored, timers: burstTimers.length,
        field: ibAbility['diagIfaceBytesBurst'] }));
  }
  restoreBurstTimerSpy();

  // IB8: source pins on the byte-identical sources. The switch is read through
  // the ONE shared readDiagSwitch; the start sits INSIDE the arm success branch
  // after the ARMED literal (the collector window's own boundary) and behind
  // its own gate; the timer is a 500 ms one bounded by the cap; the stop paths
  // (watcher stop, tick cap) call the ONE stop helper and the CLOSE EDGE is
  // pinned NOT to; the burst prefix is the new one and the probe module still
  // carries no timer.
  const burstKeyAt = extSource.indexOf("const N2H_DIAG_IFACE_BYTES_BURST_KEY: string = 'diag_iface_bytes_burst';");
  const burstReadAt = extSource.indexOf(
    'this.diagIfaceBytesBurst = this.readDiagSwitch(doc, N2H_DIAG_IFACE_BYTES_BURST_KEY);');
  const burstResetAt = extSource.indexOf('this.diagIfaceBytesBurst = false;');
  const burstStartAt = extSource.indexOf('this.startInterfaceStatsBurst();');
  const burstGateAt = extSource.indexOf('if (this.diagIfaceBytesBurst) {');
  const burstTickAt = extSource.indexOf('private interfaceStatsBurstTick(): void {');
  const burstStopFnAt = extSource.indexOf('private stopInterfaceStatsBurst(): void {');
  const burstStopClearAt = extSource.indexOf('clearInterval(this.n2hIfaceBurstTimer);');
  const burstStopAtTick = extSource.indexOf('this.stopInterfaceStatsBurst();');
  const burstStopAtWatcherStop = extSource.indexOf('this.stopInterfaceStatsBurst();',
    burstStopAtTick + 1);
  const burstStopCallSites = extSource.split('this.stopInterfaceStatsBurst();').length - 1;
  // The close edge must NOT be one of those call sites: the body of
  // maybeCloseInterfaceStatsWindow (up to the next method) still issues its own
  // close read, but carries no stop call.
  const closeFnAt = extSource.indexOf('private maybeCloseInterfaceStatsWindow(): void {');
  const burstStartFnAt = extSource.indexOf('private startInterfaceStatsBurst(): void {');
  const closeFnBody = closeFnAt > 0 && burstStartFnAt > closeFnAt
    ? extSource.slice(closeFnAt, burstStartFnAt) : '';
  check('IB8 the burst is ONE shared readDiagSwitch key, re-defaulted OFF in readDiagSwitches, ' +
    'after the other four switches',
    burstKeyAt > 0 && burstResetAt > 0 && burstReadAt > burstResetAt &&
    extSource.indexOf('this.diagPosctlUnbound = this.readDiagSwitch(') < burstReadAt,
    JSON.stringify({ burstKeyAt, burstResetAt, burstReadAt }));
  check('IB8 the start is INSIDE the arm success branch, after the ARMED literal, behind ' +
    'its own switch gate, and the interval is the 500 ms bounded one',
    okBranchAt > 0 && armedLit > okBranchAt && burstGateAt > armedLit &&
    burstStartAt > burstGateAt && elseAt > burstStartAt &&
    watcherStartAt > 0 && burstStartAt < watcherStartAt &&
    burstTickAt > burstStartAt && extSource.indexOf(
      '}, N2H_IFACE_BURST_INTERVAL_MS);') > burstStartAt &&
    extSource.indexOf("const N2H_IFACE_BURST_INTERVAL_MS: number = 500;") > 0 &&
    extSource.indexOf("const N2H_IFACE_BURST_MAX_TICKS: number = 40;") > 0 &&
    extSource.indexOf('this.n2hIfaceBurstSeq >= N2H_IFACE_BURST_MAX_TICKS') > burstTickAt,
    JSON.stringify({ armedLit, burstGateAt, burstStartAt, elseAt, burstTickAt }));
  check('IB8 the stop helper has exactly TWO call sites — the tick itself (the cap / ' +
    'destroyRequested) and the watcher stop — the close edge is pinned NOT to be one of ' +
    'them, and the helper clears the handle under an idempotent -1 guard',
    burstStopFnAt > 0 && burstStopClearAt > burstStopFnAt && burstStopCallSites === 2 &&
    burstStopAtTick > burstTickAt && burstStopAtTick < burstStopFnAt &&
    burstStopAtWatcherStop > extSource.indexOf(
      'private stopStatusAndNetcfgWatcher(): void {') &&
    burstStopAtWatcherStop < extSource.indexOf('private logConfigApplied(') &&
    closeFnBody.length > 0 &&
    closeFnBody.indexOf('this.stopInterfaceStatsBurst();') < 0 &&
    closeFnBody.indexOf('this.n2hIfaceWindowCloseDone = true;') > 0 &&
    closeFnBody.indexOf('runInterfaceStatsWindowClose(this.requestId).catch(') > 0 &&
    extSource.indexOf('if (this.n2hIfaceBurstTimer >= 0) {', burstStopFnAt) > burstStopFnAt &&
    extSource.indexOf('this.n2hIfaceBurstTimer = -1;', burstStopFnAt) > burstStopFnAt,
    JSON.stringify({ burstStopFnAt, burstStopClearAt, burstStopAtTick,
      burstStopAtWatcherStop, burstStopCallSites, closeFnLen: closeFnBody.length }));
  check('IB8 the burst line carries the NEW prefix (never the window/probe one), the fixed ' +
    'field order and the diag tail, and the probe module still owns the read with no timer',
    probeSource.indexOf(
      "'N2H_IFACE_BYTES_BURST|requestId=%{public}s|seq=%{public}s|interface=%{public}s|'") > 0 &&
    probeSource.indexOf(
      "'rxBytes=%{public}s|txBytes=%{public}s|rxError=%{public}s|txError=%{public}s|'") > 0 &&
    probeSource.indexOf(
      "'startedUnixMs=%{public}s|finishedUnixMs=%{public}s|diag=true|is_evidence=false'") > 0 &&
    probeSource.indexOf(
      'export async function readInterfaceStatsBurst(requestId: string,') > 0 &&
    probeSource.indexOf('sampleIfaceBytes(requestId, PROBE_IFACE)') > 0 &&
    probeSource.indexOf(WINDOW_PREFIX) < probeSource.indexOf(BURST_PREFIX) &&
    probeSource.indexOf('setInterval') < 0 && probeSource.indexOf('setTimeout') < 0 &&
    extSource.indexOf('n2hIfaceBurstTimer = setInterval(') > 0,
    JSON.stringify({ burstInProbe: probeSource.indexOf(BURST_PREFIX),
      windowInProbe: probeSource.indexOf(WINDOW_PREFIX) }));

  // ---------------- NC (2026-09-20): the network-context snapshot ----------
  // The PUBLIC connection getters are sampled ONCE per arm SUCCESS, in the
  // same branch as the iface probe. This is context for the parallel native
  // SO_MARK observation — NOT a routing verdict: a netId/interface/route list
  // is the system's metadata view, and nothing here decodes SO_MARK or claims
  // the socket's final path. Check shape only: values seeded into the stub
  // come out verbatim, rejections keep their code without blanking the other
  // reads, and empty worlds record empty.
  const netctxAbility = freshAbility(mod, FIXTURE_FILESDIR);
  netctxAbility['n2hAcceptedExclusionGeneration'] = 0;
  // Two nets, in the exact getAllNets order; net 101 carries two routes
  // (one without the since-20 isExcludedRoute key -> 'absent'), net 103 keeps
  // the stub default (wlan0, routes empty). The LAN-style values are stub
  // fixtures — they assert FIELD PLUMBING, never a real device's addresses.
  seedNetCtx({
    __NB_NET_DEFAULT_ID: 101,
    __NB_NET_APP_ID: 102,
    __NB_NET_ALL_IDS: [103, 101],
    __NB_NET_CAPS_BY_ID: {
      103: { networkCap: [], bearerTypes: [4] },
      101: { networkCap: [], bearerTypes: [1] }
    },
    __NB_NET_PROPS_BY_ID: {
      101: { routes: [
        { interface: 'wlan0',
          destination: { address: { address: '192.168.50.0', family: 1 }, prefixLength: 24 },
          gateway: { address: '192.168.50.1', family: 1 }, hasGateway: true,
          isDefaultRoute: false },
        { interface: 'wlan0',
          destination: { address: { address: '0.0.0.0', family: 1 }, prefixLength: 0 },
          gateway: { address: '192.168.50.1', family: 1 }, hasGateway: true,
          isDefaultRoute: true }
      ] }
    }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(netctxAbility, WG_READY);
  await drainProbe();
  const nc1Lines = linesWith(NETCTX_LINE);
  const nc1 = nc1Lines.length === 1 ? nc1Lines[0] : '';
  check('NC1 the armed snapshot carries seeded default/app netIds, the list verbatim, ' +
    'per-net iface/bearer/routes, and digit begin/end',
    nc1Lines.length === 1 &&
    nc1 === 'N2H_NET_CONTEXT|requestId=' + REQUEST_ID +
    '|defaultNetId=101|defaultNetError=none|appNetId=102|appNetError=none|' +
    'nets=2|allError=none|truncated=false|netsDetail=' +
    'netId=103,iface=wlan0,bearer=4,capsError=none,propsError=none,routes=empty;' +
    'netId=101,iface=wlan0,bearer=1,capsError=none,propsError=none,routes=' +
    'dest~192.168.50.0~prefix~24~excl~absent~gw~192.168.50.1+' +
    'dest~0.0.0.0~prefix~0~excl~absent~gw~192.168.50.1' +
    '|startedUnixMs=' + fieldOf(nc1, 'startedUnixMs') +
    '|finishedUnixMs=' + fieldOf(nc1, 'finishedUnixMs') +
    '|diag=true|is_evidence=false' &&
    /^\d+$/.test(fieldOf(nc1, 'startedUnixMs')) &&
    /^\d+$/.test(fieldOf(nc1, 'finishedUnixMs')) &&
    Number(fieldOf(nc1, 'finishedUnixMs')) >= Number(fieldOf(nc1, 'startedUnixMs')),
    JSON.stringify(nc1Lines));

  // NC2: one rejected API keeps its code as 'unavailable' while every
  // independent read still records — appNet rejects 201, net 101's props
  // reject 201, and defaultNet / the full list / net 103's segment / net
  // 101's bearer all still come through.
  const nc2Ability = freshAbility(mod, FIXTURE_FILESDIR);
  nc2Ability['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_DEFAULT_ID: 101,
    __NB_NET_THROW_CODES: { getAppNetSync: 201 },
    __NB_NET_ALL_IDS: [103, 101],
    __NB_NET_PROPS_BY_ID: { 101: { error: 201 } }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(nc2Ability, WG_READY);
  await drainProbe();
  const nc2Lines = linesWith(NETCTX_LINE);
  const nc2 = nc2Lines.length === 1 ? nc2Lines[0] : '';
  check('NC2 a rejected API leaves its field unavailable with the code; the other ' +
    'independent reads still record',
    nc2Lines.length === 1 &&
    nc2 === 'N2H_NET_CONTEXT|requestId=' + REQUEST_ID +
    '|defaultNetId=101|defaultNetError=none|appNetId=unavailable|appNetError=201|' +
    'nets=2|allError=none|truncated=false|netsDetail=' +
    'netId=103,iface=wlan0,bearer=empty,capsError=none,propsError=none,routes=empty;' +
    'netId=101,iface=unavailable,bearer=empty,capsError=none,propsError=201,' +
    'routes=unavailable' +
    '|startedUnixMs=' + fieldOf(nc2, 'startedUnixMs') +
    '|finishedUnixMs=' + fieldOf(nc2, 'finishedUnixMs') +
    '|diag=true|is_evidence=false',
    JSON.stringify(nc2Lines));

  // NC3: the empty handle world — 0 / 0 / an empty list record verbatim as
  // '0'/'0'/nets=0/detail=empty. No physical/VPN selection story is invented.
  const nc3Ability = freshAbility(mod, FIXTURE_FILESDIR);
  nc3Ability['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({});
  setArmFile(true, undefined);
  resetLog();
  tickArm(nc3Ability, WG_READY);
  await drainProbe();
  const nc3Lines = linesWith(NETCTX_LINE);
  const nc3 = nc3Lines.length === 1 ? nc3Lines[0] : '';
  check('NC3 an empty handle world records 0/0/0 verbatim (detail=empty), no invented story',
    nc3Lines.length === 1 &&
    nc3.indexOf('N2H_NET_CONTEXT|requestId=' + REQUEST_ID +
      '|defaultNetId=0|defaultNetError=none|appNetId=0|appNetError=none|' +
      'nets=0|allError=none|truncated=false|netsDetail=empty|startedUnixMs=') === 0 &&
    nc3.endsWith('|diag=true|is_evidence=false'),
    JSON.stringify(nc3Lines));

  // NC4: only a real arm SUCCESS triggers the snapshot — a closed gate fires
  // nothing, a core not-ok answers nothing, and the armed instance never
  // repeats (one-shot inherited from the arm latch).
  const nc4Gate = freshAbility(mod, FIXTURE_FILESDIR);
  nc4Gate['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({ __NB_NET_ALL_IDS: [103] });
  setArmFile(true, undefined);
  resetLog();
  tickArm(nc4Gate, { wg: { device_up: true, ready: false } });
  const nc4CallsAtWait = linesWith(NETCTX_LINE).length;
  const nc4Refused = freshAbility(mod, FIXTURE_FILESDIR);
  nc4Refused['n2hAcceptedExclusionGeneration'] = 0;
  G['__NB_ARM_ANSWER'] = '{"ok":false,"error":"forced-by-nc4"}';
  tickArm(nc4Refused, WG_READY);
  G['__NB_ARM_ANSWER'] = undefined;
  await drainProbe();
  check('NC4 no ARM success fires nothing (gate closed, core not-ok) — zero snapshot lines',
    nc4CallsAtWait === 0 &&
    linesWith('N2H_COLLECTOR_ARM_FAILED|').length === 1 &&
    linesWith(NETCTX_LINE).length === 0 &&
    linesWith(PROBE_PREFIX).length === 0, JSON.stringify(logs()));
  resetLog();
  tickArm(netctxAbility, WG_READY);
  check('NC4 ticks after the arm repeat nothing (one-shot inherited)',
    logs().length === 0, JSON.stringify(logs()));

  // NC5: source pins — the snapshot stays getter-only metadata (no
  // bindSocket/setAppNet/setNet*, no exception message text), still shares the
  // byte-only module, rides the same ok-branch AFTER the iface probe, and the
  // last-resort aborted guard prints codes/names only.
  check('NC5 the snapshot is getter-only metadata in the shared probe module',
    probeSource.indexOf(NETCTX_LINE) > 0 &&
    probeSource.indexOf('bindSocket') < 0 &&
    probeSource.indexOf('setAppNet') < 0 &&
    probeSource.indexOf('setNet') < 0 &&
    probeSource.indexOf('.message') < 0 &&
    probeSource.indexOf('getIfaceRxBytes') > 0,
    'net-context module pin');
  const ncCall = extSource.indexOf('runNetworkContextProbe(this.requestId)');
  check('NC5 the snapshot call rides the same ok-branch, after the iface probe',
    ncCall > extSource.indexOf('runInterfaceStatsProbe(this.requestId, (): void => {') &&
    ncCall > armedLit && ncCall < elseAt,
    JSON.stringify({ armedLit, elseAt, ncCall }));
  check('NC5 the aborted guard prints codes/names only (never an exception message)',
    extSource.indexOf(
      "'N2H_NET_CONTEXT_ABORTED|requestId=%{public}s|code=%{public}s|'") > 0 &&
    extSource.indexOf("'name=%{public}s|diag=true|is_evidence=false',") > 0,
    'net-context aborted-guard pin');

  // ---------------- PB (2026-09-20): the posctl probe netId ----------------
  // The arm resolves the VPN network's netId via the OFFICIAL read-only
  // getters BEFORE the native arm and puts the non-secret integer into the
  // opts JSON ONLY when exactly one vpn-tun handle was found; every other
  // outcome keeps the key ABSENT (the probe stays byte-identical) and one
  // marker carries the reason + error CODE token. No bind ever happens in
  // the shell: the stub module has no bindSocket/setAppNet at all, so any
  // new call site would be a loud TypeError, and the source pins below keep
  // it structural.
  const PROBE_NET_MARKER = 'N2H_POSCTL_PROBE_NET_ID_UNAVAILABLE|';

  function armOptsJson(): Record<string, unknown> {
    const raw = G['__NB_ARM_OPTS'] as string | undefined;
    return raw === undefined ? {} : JSON.parse(raw) as Record<string, unknown>;
  }

  // PB1: found — one vpn-tun handle (net 147) lands in the opts JSON with
  // the other fields unchanged; the arm still answers ok exactly once.
  const pb1 = freshAbility(mod, FIXTURE_FILESDIR);
  pb1['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_ALL_IDS: [147, 101],
    __NB_NET_PROPS_BY_ID: { 147: { interfaceName: 'vpn-tun' } }
  });
  setArmFile(true, undefined);
  resetLog();
  const pb1ArmCallsAt = Number(G['__NB_ARM_CALLS'] ?? 0);
  tickArm(pb1, WG_READY);
  await drainProbe();
  const pb1Opts = armOptsJson();
  check('PB1 the found vpn-tun netId rides the opts JSON verbatim, other fields unchanged',
    pb1Opts['probe_net_id'] === 147 &&
    pb1Opts['exclusion_generation'] === 0 &&
    typeof pb1Opts['posctl_port'] === 'number' && pb1Opts['posctl_port'] > 0 &&
    typeof pb1Opts['probe_wait_ms'] === 'number' && pb1Opts['probe_wait_ms'] > 0 &&
    Array.isArray(pb1Opts['shell_exclusions']),
    JSON.stringify(pb1Opts));
  check('PB1 the arm still answers ok exactly once and no unavailability marker fires',
    Number(G['__NB_ARM_CALLS']) === pb1ArmCallsAt + 1 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 1 &&
    linesWith(PROBE_NET_MARKER).length === 0,
    JSON.stringify(logs()));

  // PB2: not found — the default wlan0-only world keeps the key ABSENT and
  // records exactly ONE unavailability marker (reason only, code=none).
  const pb2 = freshAbility(mod, FIXTURE_FILESDIR);
  pb2['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({ __NB_NET_ALL_IDS: [103] });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb2, WG_READY);
  await drainProbe();
  const pb2Opts = armOptsJson();
  const pb2Markers = linesWith(PROBE_NET_MARKER);
  check('PB2 no vpn-tun handle keeps the key absent and prints ONE reason marker (not-found)',
    !('probe_net_id' in pb2Opts) &&
    (G['__NB_ARM_OPTS'] as string).indexOf('probe_net_id') < 0 &&
    pb2Markers.length === 1 &&
    pb2Markers[0].indexOf('reason=not-found|code=none|diag=true|is_evidence=false') > 0 &&
    linesWith('N2H_COLLECTOR_ARMED|').length === 1,
    JSON.stringify({ opts: pb2Opts, markers: pb2Markers }));

  // PB3: the list read rejects — key absent, ONE marker with the raw code.
  const pb3 = freshAbility(mod, FIXTURE_FILESDIR);
  pb3['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({ __NB_NET_THROW_CODES: { getAllNetsSync: 201 } });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb3, WG_READY);
  await drainProbe();
  const pb3Markers = linesWith(PROBE_NET_MARKER);
  check('PB3 a rejected getAllNetsSync keeps the key absent and prints the code (list-error)',
    !('probe_net_id' in armOptsJson()) &&
    pb3Markers.length === 1 &&
    pb3Markers[0].indexOf('reason=list-error|code=201|diag=true|is_evidence=false') > 0,
    JSON.stringify(pb3Markers));

  // PB4: a name collision — two vpn-tun handles refuse the resolution (never
  // a guess by list order); key absent, ONE marker reason=ambiguous.
  const pb4 = freshAbility(mod, FIXTURE_FILESDIR);
  pb4['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_ALL_IDS: [147, 148],
    __NB_NET_PROPS_BY_ID: {
      147: { interfaceName: 'vpn-tun' },
      148: { interfaceName: 'vpn-tun' }
    }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb4, WG_READY);
  await drainProbe();
  const pb4Markers = linesWith(PROBE_NET_MARKER);
  check('PB4 two vpn-tun handles are ambiguous: key absent, ONE marker, never a list-order pick',
    !('probe_net_id' in armOptsJson()) &&
    pb4Markers.length === 1 &&
    pb4Markers[0].indexOf('reason=ambiguous|code=none|') > 0,
    JSON.stringify(pb4Markers));

  // PB5: the vpn-tun handle's props reject — the handle is skipped, the
  // world answers not-found (the code stays with the marker, never a value).
  const pb5 = freshAbility(mod, FIXTURE_FILESDIR);
  pb5['n2hAcceptedExclusionGeneration'] = 0;
  seedNetCtx({
    __NB_NET_ALL_IDS: [147],
    __NB_NET_PROPS_BY_ID: { 147: { error: 201 } }
  });
  setArmFile(true, undefined);
  resetLog();
  tickArm(pb5, WG_READY);
  await drainProbe();
  const pb5Markers = linesWith(PROBE_NET_MARKER);
  check('PB5 an unreadable vpn-tun handle is skipped: key absent, ONE marker (not-found)',
    !('probe_net_id' in armOptsJson()) &&
    pb5Markers.length === 1 &&
    pb5Markers[0].indexOf('reason=not-found|code=none|') > 0,
    JSON.stringify(pb5Markers));

  // PB6: source pins — the extension imports connection to observe physical
  // network changes, but never binds sockets or samples netId facts directly:
  // those still flow through the sibling getter-only module (NC5 pins).
  // The resolution sits BEFORE the native arm call, the only opts write is
  // the found branch, and the marker prints codes only.
  check('PB6 connection import is observer-only; netId facts stay in the getter sibling',
    extSource.includes("import connection from '@ohos.net.connection';") &&
    extSource.includes('connection.createNetConnection()') &&
    extSource.includes('resolveVpnTunNetId') &&
    !/connection\.(?:getDefaultNetSync|getAppNetSync|getAllNetsSync|getNetCapabilitiesSync|getConnectionPropertiesSync|bindSocket|bindAppNetwork)\s*\(/.test(extSource),
    'extension netId pin');
  const pbResolveAt = extSource.indexOf('resolveVpnTunNetId()');
  const pbArmAt = extSource.indexOf('core.n2h_collector_arm(optsJson)');
  check('PB6 the resolution runs BEFORE the native arm call and writes only on found',
    pbResolveAt > 0 && pbArmAt > pbResolveAt &&
    extSource.indexOf('opts.probe_net_id = vpnResolution.netId;') > pbResolveAt &&
    extSource.indexOf("'N2H_POSCTL_PROBE_NET_ID_UNAVAILABLE|requestId=%{public}s|reason=%{public}s|'") > 0,
    JSON.stringify({ pbResolveAt, pbArmAt }));
  check('PB6 no permission surface was added (module.json5 requestPermissions untouched)',
    readFileSync(join(HERE, '..', 'src', 'main', 'module.json5')).toString()
      .indexOf('GET_NETWORK_STATS') < 0,
    'module.json5 pin');

  // ---------------- D (2026-09-23): the S2-S4 diag switches and the
  // ---------------- DEFAULT tunnel-subnet route (2026-09-26) ---------------
  // Three DEFAULT-OFF diagnostic switches riding the SAME arm ticket
  // (netbird-n2h-arm): S2 diag_blocked_self (blockedApplications = this
  // bundle), S3 diag_protect_process_net (one real protectProcessNet() call)
  // and S4 diag_posctl_unbound (the posctl probe gets no probe_net_id). The
  // former S1 diag_tunnel_route is RETIRED: the tunnel-subnet route is now
  // declared in VpnConfig.routes UNCONDITIONALLY on BOTH legs, and a ticket
  // that still carries the key is an unknown key — silently ignored.
  //
  // The D group drives the REAL code end-to-end:
  //   - the INITIAL leg through the REAL startConnectorThenCreate (credentials
  //     read from the fixture filesDir, the REAL DNS/parse/exclusion
  //     derivation, the REAL route logs + validation gate, the REAL
  //     buildPlatformVpnConfig and the FakeConnection's create()), so "the
  //     submitted VpnConfig" is a captured platform document, not a mirror:
  //     every capture is compared field-by-field (JSON.stringify equality)
  //     against the SAME real pure composition — that is what "identical to
  //     the documented default behavior" means here;
  //   - the RECREATE leg through the REAL performControlledRecreate
  //     (driveRecreate, now with a /16 netcfg variant);
  //   - the S3 call site through the REAL settleCreate;
  //   - the S4 branch through the REAL arm hook;
  //   - the tunnel-subnet network computation through the REAL pure function
  //     directly.
  const DIAG_PREFIX = 'N2H_DIAG_';
  const DIAG_IGNORED = 'N2H_DIAG_OPTION_IGNORED|';
  const TUNNEL_SUBNET_ROUTE = 'VPN_TUNNEL_SUBNET_ROUTE|';
  const DIAG_BLOCKED_APPS = 'N2H_DIAG_BLOCKED_APPS|';
  const DIAG_UNBOUND = 'N2H_DIAG_POSCTL_UNBOUND|';
  const PROCESS_NET = 'VPN_PROTECT_PROCESS_NET|';
  const SELF_BUNDLE = 'cn.alfadb.netbird';

  function deepEq(a: unknown, b: unknown): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
  }

  function diagLines(): string[] {
    return logs().filter((l: string): boolean => l.indexOf(DIAG_PREFIX) >= 0)
      .map((l: string): string => l.slice(l.indexOf(DIAG_PREFIX)));
  }

  function removeFile(path: string): void {
    const files = (G['__NB_FS'] ?? {}) as Record<string, string>;
    delete files[path];
    G['__NB_FS'] = files;
  }

  // The switch ticket as EntryAbility would have written it: `undefined` =
  // NO ticket at all (the structural default: the launcher carried no
  // n2h_arm_options parameter, so the ticket was deleted).
  function setSwitchTicket(text: string | undefined): void {
    setArmFile(text === undefined ? false : true, undefined, text === undefined ? '' : text);
  }

  // The 0021-shaped initial map: tunnel address 100.108.144.165/16 (a REAL
  // device prefix — the S1 network of the register is 100.108.0.0/16), one
  // managed /24, no default route released, and a protected signal endpoint.
  function initialNetcfg(routes: Array<Record<string, unknown>>): string {
    return JSON.stringify({
      available: true,
      serial: 5,
      address: '100.108.144.165',
      address_prefix_len: 16,
      routes: routes,
      dns: { service_enable: true, servers: [{ ip: '9.9.9.9', port: 53 }] },
      default_route: { allowed: false, reason: 'data-plane-not-ready' },
      signal: 'https://mgmt.test:443',
      stuns: []
    });
  }

  const NETCFG_0021 = initialNetcfg([{ network: '10.10.10.0/24', is_default: false }]);

  // The recreate leg's netcfg with a REAL /16 tunnel address (netcfgJson's
  // 10.99.0.1/32 would take the S1 /32 skip path; both are exercised below).
  function netcfgRecreate16(): string {
    return JSON.stringify({
      available: true,
      serial: 9,
      address: '100.108.144.165',
      address_prefix_len: 16,
      routes: [{ network: '172.16.0.0/12', is_default: false }],
      dns: { service_enable: true, servers: [{ ip: '8.8.8.8', port: 53 }] },
      default_route: { allowed: true, reason: 'wg-tunnel-ready' },
      stuns: []
    });
  }

  // The REAL initial-leg composition with NO switch applied: the same real
  // pure functions in the same order, with the SAME frozen exclusion set the
  // driven leg retained and the SAME snapshot. `gateAllowed` is the real
  // verdict (net.default_route.allowed || forceDefaultRoute=false).
  function legacyInitialTunnelConfig(ability: Record<string, unknown>,
    netcfgText: string): Record<string, unknown> {
    const base = (cfg['withEndpointExclusions'] as
      (b: unknown, e: unknown[]) => Record<string, unknown>)(
      cfg['loadVpnTunnelConfig'](), ability['frozenEndpointExclusions'] as unknown[]);
    const merged = (cfg['applyNetworkConfig'] as
      (b: unknown, s: unknown) => Record<string, unknown>)(base, JSON.parse(netcfgText) as never);
    return (cfg['gateDefaultRoutes'] as
      (c: unknown, a: boolean) => Record<string, unknown>)(merged, false);
  }

  function legacyRecreateTunnelConfig(frozen: unknown[],
    netcfgText: string): Record<string, unknown> {
    const base = (cfg['withEndpointExclusions'] as
      (b: unknown, e: unknown[]) => Record<string, unknown>)(
      cfg['loadVpnTunnelConfig'](), frozen);
    const snapshot = JSON.parse(netcfgText) as Record<string, unknown>;
    const merged = (cfg['applyNetworkConfig'] as
      (b: unknown, s: unknown) => Record<string, unknown>)(base, snapshot);
    const allowed = ((snapshot['default_route'] as Record<string, unknown>)['allowed']) === true;
    return (cfg['gateDefaultRoutes'] as
      (c: unknown, a: boolean) => Record<string, unknown>)(merged, allowed);
  }

  function platformOf(tunnel: Record<string, unknown>,
    blocked?: string[]): Record<string, unknown> {
    return (cfg['buildPlatformVpnConfig'] as
      (c: unknown, b?: string[]) => Record<string, unknown>)(tunnel, blocked);
  }

  function injectedOf(tunnel: Record<string, unknown>): Record<string, unknown> {
    return (cfg['withTunnelSubnetRoute'] as
      (c: unknown) => Record<string, unknown>)(tunnel);
  }

  // The vpnExtension stub's OWN FakeConnection (the hooks file's class, whose
  // protectProcessNet drives the S3 groups). The resolve hook maps the
  // specifier for this run file too, so the very class the extension's
  // createVpnConnection() hands out is reachable here.
  const fakeVpnNs = (await import('@ohos.net.vpnExtension') as Record<string, unknown>)['default'] as
    Record<string, unknown>;
  const FakeConnection = fakeVpnNs['VpnConnection'] as new () => Record<string, unknown>;

  // Drive ONE full initial leg through the REAL startConnectorThenCreate.
  // Everything the flow needs is seeded: the fixture credentials in filesDir,
  // the management DNS answer, the core snapshot, the relay view (none) and a
  // clean network-context world. The protect-deviation file is REMOVED (a
  // leftover from the C group would silently arm the deviation); the ticket is
  // whatever the caller seeded. Returns the platform configs the FakeConnection
  // captured (the create() submissions).
  async function driveInitialLeg(ability: Record<string, unknown>,
    netcfgText: string): Promise<Record<string, unknown>[]> {
    G['__NB_NETCFG'] = netcfgText;
    G['__NB_STATUS'] = { relay: { urls: [] } };
    G['__NB_DNS'] = { 'mgmt.test': '203.0.113.7' };
    G['__NB_FD_STATUS'] = { open: true, errno: 0 };
    G['__NB_MGMT_FD'] = 21;
    G['__NB_CREATED_CONFIGS'] = [];
    seedNetCtx({});
    removeFile(`${FIXTURE_FILESDIR}/netbird-protect-deviation`);
    seedFile(`${FIXTURE_FILESDIR}/netbird-setup-key`, 'setup-key-body');
    seedFile(`${FIXTURE_FILESDIR}/netbird-device-config.json`,
      JSON.stringify({ management_url: 'http://mgmt.test:33073', private_key: 'pk-body' }));
    const connection = new FakeConnection();
    ability['connection'] = connection;
    await (ability['startConnectorThenCreate'] as
      (w: unknown, c: unknown) => Promise<void>)({ parameters: {} }, connection);
    await drainProbe(); // let the create() -> settleCreate chain run
    return (G['__NB_CREATED_CONFIGS'] ?? []) as Record<string, unknown>[];
  }

  // The create() submission of ONE recreate leg.
  async function driveRecreateCapture(ability: Record<string, unknown>,
    netcfgText: string): Promise<Record<string, unknown>[]> {
    G['__NB_CREATED_CONFIGS'] = [];
    await driveRecreate(ability, netcfgText);
    return (G['__NB_CREATED_CONFIGS'] ?? []) as Record<string, unknown>[];
  }

  // ---- D0: wiring pins + "the diag keys never reach the core" -------------
  const diagReadAt = extSource.indexOf('this.readDiagSwitches();');
  const subnetInitialAt = extSource.indexOf("this.injectTunnelSubnetRoute(merged, 'initial')");
  const diagLogInitialAt = extSource.indexOf(
    'this.logExclusionGeneration(EXCLUSION_GENERATION_INITIAL, merged.routes, 0);');
  const subnetRecreateAt = extSource.indexOf("this.injectTunnelSubnetRoute(merged, 'recreate')");
  const diagLogRecreateAt = extSource.indexOf(
    'this.logExclusionGeneration(EXCLUSION_GENERATION_RECREATE, merged.routes, 0);');
  check('D0 the switches are read BEFORE the first config build and the tunnel-subnet route ' +
    'lands before BOTH route dumps, on BOTH legs',
    diagReadAt > 0 && diagReadAt < subnetInitialAt && subnetInitialAt < diagLogInitialAt &&
    subnetRecreateAt > 0 && subnetRecreateAt < diagLogRecreateAt,
    JSON.stringify({ diagReadAt, subnetInitialAt, diagLogInitialAt, subnetRecreateAt,
      diagLogRecreateAt }));
  check('D0 the tunnel-subnet route has exactly the TWO UNCONDITIONAL call sites ' +
    '(initial + recreate) and every piece of the retired S1 switch is gone',
    extSource.split('this.injectTunnelSubnetRoute(merged,').length === 3 &&
    extSource.indexOf('if (this.diagTunnelRoute) {') < 0 &&
    extSource.indexOf('diagTunnelRoute') < 0 &&
    extSource.indexOf('N2H_DIAG_TUNNEL_ROUTE') < 0, 'tunnel-subnet call-site pin');
  check('D0 S2 rides buildPlatformVpnConfig at BOTH legs via the ONE list decider',
    extSource.split('this.diagBlockedApplicationList(').length === 3 &&
    extSource.split('buildPlatformVpnConfig(merged, this.diagBlockedApplicationList(').length === 3,
    'S2 call-site pin');
  const s3AnchorAt = extSource.indexOf(
    "'VPN_CREATE_RESOLVED|requestId=%{public}s|fd=%{public}s|accepted=%{public}s|marker=%{public}s'");
  const s3CallAt = extSource.indexOf('this.protectProcessNetWithBox(processConnection)');
  const s3GateAt = extSource.indexOf(
    'const protectedOk: boolean = await this.protectOutboundSocketFailClosed();');
  check('D0 S3 sits between the acceptance anchor and the fail-closed protect gate, behind the latch',
    s3AnchorAt > 0 && s3AnchorAt < s3CallAt && s3CallAt < s3GateAt &&
    extSource.indexOf('if (this.diagProtectProcessNet && !this.processProtectAttempted) {') > 0 &&
    extSource.split('protectProcessNetWithBox(').length === 3, 'S3 ordering pin');
  check('D0 S4 gates only the opts write, after the resolution, and never the UNAVAILABLE line',
    extSource.indexOf('if (this.diagPosctlUnbound) {') > 0 &&
    extSource.indexOf('if (this.diagPosctlUnbound) {') <
    extSource.indexOf('opts.probe_net_id = vpnResolution.netId;') &&
    extSource.indexOf('N2H_DIAG_POSCTL_UNBOUND|requestId=%{public}s|diag=true|is_evidence=false') > 0,
    'S4 branch pin');
  // Behavioral: an ALL-ON ticket still forwards ONLY the documented arm keys
  // (none of the diag keys has a path into the opts document). The retired
  // `diag_tunnel_route` key is still carried here on purpose: it must leave no
  // instance field and print nothing (unknown key, silent ignore).
  const d0 = freshAbility(mod, FIXTURE_FILESDIR);
  d0['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, JSON.stringify({
    diag_tunnel_route: true, diag_blocked_self: true,
    diag_protect_process_net: true, diag_posctl_unbound: true
  }));
  (d0['readDiagSwitches'] as () => void)();
  seedNetCtx({ __NB_NET_ALL_IDS: [147], __NB_NET_PROPS_BY_ID: { 147: { interfaceName: 'vpn-tun' } } });
  resetLog();
  tickArm(d0, WG_READY);
  await drainProbe();
  const d0Opts = armOptsJson();
  check('D0 an all-on ticket forwards NONE of the diag keys to the core (opts = the documented set)',
    deepEq(Object.keys(d0Opts).sort(),
      ['exclusion_generation', 'posctl_port', 'probe_wait_ms', 'shell_exclusions']) &&
    d0Opts['probe_net_id'] === undefined &&
    d0['diagTunnelRoute'] === undefined &&
    d0['diagBlockedSelf'] === true &&
    d0['diagProtectProcessNet'] === true && d0['diagPosctlUnbound'] === true,
    JSON.stringify(d0Opts));

  // ---- D1①: NO ticket at all = the DEFAULT behavior, BOTH legs ------------
  // The tunnel-subnet route is no longer a switch: with an absent ticket the
  // submission is the legacy composition PLUS exactly that one route.
  const d1 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(undefined);
  resetLog();
  const d1Submitted = await driveInitialLeg(d1, NETCFG_0021);
  const d1LegacyTunnel = legacyInitialTunnelConfig(d1, NETCFG_0021);
  const d1Tunnel = injectedOf(d1LegacyTunnel);
  check('D1 initial leg: the ticketless drive really submitted ONE VpnConfig',
    d1Submitted.length === 1, JSON.stringify({ count: d1Submitted.length, logs: logs() }));
  check('D1 initial leg: field-by-field identical to the legacy composition PLUS the default ' +
    'tunnel-subnet route (exactly one row more)',
    deepEq(d1Submitted[0], platformOf(d1Tunnel['config'])) &&
    ((d1Submitted[0] ?? {})['routes'] as unknown[]).length ===
    ((d1LegacyTunnel['routes'] ?? []) as unknown[]).length + 1,
    JSON.stringify({ submitted: d1Submitted[0], expected: platformOf(d1Tunnel['config']) }));
  const d1Routes = (d1Submitted[0] ?? {})['routes'] as Array<Record<string, unknown>>;
  const d1Subnet = (d1Routes ?? []).filter((r: Record<string, unknown>): boolean =>
    ((r['destination'] as Record<string, unknown>)['address'] as Record<string, unknown>)['address'] ===
    '100.108.0.0');
  check('D1 initial leg: the default tunnel-subnet entry is exactly the managed shape ' +
    '(100.108.0.0/16, gateway = the tunnel address, non-default, non-excluded, vpn-tun)',
    d1Subnet.length === 1 && deepEq(d1Subnet[0], {
      interface: 'vpn-tun',
      destination: { address: { address: '100.108.0.0', family: 1 }, prefixLength: 16 },
      gateway: { address: '100.108.144.165', family: 1 },
      hasGateway: true,
      isDefaultRoute: false
    }), JSON.stringify(d1Routes));
  check('D1 initial leg: the submitted document has EXACTLY the four legacy fields ' +
    '(no blockedApplications / no trustedApplications / no fifth key)',
    deepEq(Object.keys(d1Submitted[0] ?? {}),
      ['addresses', 'routes', 'dnsAddresses', 'mtu']) &&
    !('blockedApplications' in (d1Submitted[0] ?? {})) &&
    !('trustedApplications' in (d1Submitted[0] ?? {})),
    JSON.stringify(Object.keys(d1Submitted[0] ?? {})));
  check('D1 initial leg: the tunnel-subnet marker is the fixed literal (leg=initial, ' +
    'result=added, reason=none, no diag tail)',
    lastLine(TUNNEL_SUBNET_ROUTE) === 'VPN_TUNNEL_SUBNET_ROUTE|requestId=' + REQUEST_ID +
    '|addr=100.108.144.165|prefix=16|network=100.108.0.0|leg=initial|result=added|reason=none',
    lastLine(TUNNEL_SUBNET_ROUTE));
  check('D1 initial leg: NOTHING diagnostic was logged (no N2H_DIAG_ line at all)',
    diagLines().length === 0 && Number(G['__NB_PROCESS_PROTECT_CALLS'] ?? 0) === 0,
    JSON.stringify(diagLines()));

  const d1r = freshAbility(mod, FIXTURE_FILESDIR);
  d1r['frozenEndpointExclusions'] = derived;
  resetLog();
  const d1rSubmitted = await driveRecreateCapture(d1r, netcfgRecreate16());
  const d1rLegacyTunnel = legacyRecreateTunnelConfig(derived, netcfgRecreate16());
  const d1rTunnel = injectedOf(d1rLegacyTunnel);
  check('D1 recreate leg: field-by-field identical to the legacy composition PLUS the default ' +
    'tunnel-subnet route (the rebuilt VpnConfig is fully replaced, so the entry must be ' +
    're-declared)',
    d1rSubmitted.length === 1 &&
    deepEq(d1rSubmitted[0], platformOf(d1rTunnel['config'])) &&
    ((d1rSubmitted[0] ?? {})['routes'] as unknown[]).length ===
    ((d1rLegacyTunnel['routes'] ?? []) as unknown[]).length + 1,
    JSON.stringify({ submitted: d1rSubmitted[0], expected: platformOf(d1rTunnel['config']) }));
  check('D1 recreate leg: no blockedApplications field, no N2H_DIAG_ line, no protectProcessNet ' +
    'call, and the default marker says leg=recreate',
    !('blockedApplications' in (d1rSubmitted[0] ?? {})) &&
    !('trustedApplications' in (d1rSubmitted[0] ?? {})) &&
    diagLines().length === 0 && Number(G['__NB_PROCESS_PROTECT_CALLS'] ?? 0) === 0 &&
    lastLine(TUNNEL_SUBNET_ROUTE) === 'VPN_TUNNEL_SUBNET_ROUTE|requestId=' + REQUEST_ID +
    '|addr=100.108.144.165|prefix=16|network=100.108.0.0|leg=recreate|result=added|reason=none',
    JSON.stringify(diagLines()));

  // S4 default: with the switch ABSENT the probe_net_id still rides (PB1's
  // behavior is unchanged by the new branch). The ticket must carry SOMETHING
  // (an empty/absent ticket never arms at all), so it carries the pre-existing
  // non-switch key only.
  const d1s4 = freshAbility(mod, FIXTURE_FILESDIR);
  d1s4['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(JSON.stringify({ posctl_port: 41000 }));
  seedNetCtx({ __NB_NET_ALL_IDS: [147], __NB_NET_PROPS_BY_ID: { 147: { interfaceName: 'vpn-tun' } } });
  resetLog();
  tickArm(d1s4, WG_READY);
  await drainProbe();
  check('D1 S4 off: probe_net_id is still passed and no D-markers of any kind appear',
    armOptsJson()['probe_net_id'] === 147 && diagLines().length === 0,
    JSON.stringify(armOptsJson()));

  // ---- D2②: the DEFAULT tunnel-subnet route, BOTH legs --------------------
  // The ticket still carries the RETIRED `diag_tunnel_route` key on purpose:
  // it must be an inert unknown key, so this drive must submit bit-for-bit the
  // SAME document as the ticketless D1 drive.
  const d2 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({ diag_tunnel_route: true }));
  resetLog();
  const d2Submitted = await driveInitialLeg(d2, NETCFG_0021);
  const d2Gated = legacyInitialTunnelConfig(d2, NETCFG_0021);
  const d2Injected = injectedOf(d2Gated);
  check('D2 initial leg: the submitted VpnConfig IS the pure composition ' +
    '(legacy config + the tunnel-subnet route)',
    d2Submitted.length === 1 && deepEq(d2Submitted[0], platformOf(d2Injected['config'])),
    JSON.stringify(d2Submitted[0]));
  check('D2 a ticket carrying the RETIRED "diag_tunnel_route":true key behaves EXACTLY like no ' +
    'ticket at all (same submitted document, no ignored-key line)',
    deepEq(d2Submitted[0], d1Submitted[0]) && diagLines().length === 0,
    JSON.stringify({ d2: d2Submitted[0], d1: d1Submitted[0], diag: diagLines() }));
  const d2Routes = (d2Submitted[0] ?? {})['routes'] as Array<Record<string, unknown>>;
  const d2Subnet = d2Routes.filter((r: Record<string, unknown>): boolean =>
    ((r['destination'] as Record<string, unknown>)['address'] as Record<string, unknown>)['address'] ===
    '100.108.0.0');
  check('D2 initial leg: the tunnel subnet entry is exactly the managed shape ' +
    '(100.108.0.0/16, gateway = the tunnel address, non-default, non-excluded, vpn-tun)',
    d2Subnet.length === 1 && deepEq(d2Subnet[0], {
      interface: 'vpn-tun',
      destination: { address: { address: '100.108.0.0', family: 1 }, prefixLength: 16 },
      gateway: { address: '100.108.144.165', family: 1 },
      hasGateway: true,
      isDefaultRoute: false
    }), JSON.stringify(d2Routes));
  check('D2 initial leg: the generation marker routes= and the route rows equal the SUBMITTED routes',
    fieldOf(lastLine('VPN_EXCLUSION_GENERATION|'), 'routes') === String(d2Routes.length) &&
    lastLine('VPN_EXCLUSION_GENERATION|').indexOf('|kind=initial|') > 0 &&
    linesWith('VPN_ROUTE_ENTRY|').length === d2Routes.length,
    lastLine('VPN_EXCLUSION_GENERATION|') + ' rows=' +
    String(linesWith('VPN_ROUTE_ENTRY|').length));
  check('D2 initial leg: the tunnel-subnet marker is the fixed literal ' +
    '(leg=initial, added/none, no diag tail)',
    lastLine(TUNNEL_SUBNET_ROUTE) === 'VPN_TUNNEL_SUBNET_ROUTE|requestId=' + REQUEST_ID +
    '|addr=100.108.144.165|prefix=16|network=100.108.0.0|leg=initial|result=added|reason=none',
    lastLine(TUNNEL_SUBNET_ROUTE));
  const d2GatedAfter = (cfg['gateDefaultRoutes'] as
    (c: unknown, a: boolean) => Record<string, unknown>)(d2Injected['config'], false);
  const d2GatedRoutes = (d2GatedAfter['routes'] ?? []) as Array<Record<string, unknown>>;
  check('D2 initial leg: gateDefaultRoutes(false) does NOT swallow the tunnel-subnet entry and ' +
    'the fail-closed validation gate accepted it (the create really ran)',
    d2GatedRoutes.length === ((d2Injected['config'] as Record<string, unknown>)['routes'] as
      unknown[]).length &&
    d2GatedRoutes.filter((r: Record<string, unknown>): boolean =>
      r['destination'] === '100.108.0.0').length === 1 &&
    linesWith('VPN_ROUTE_VIOLATION|').length === 0,
    JSON.stringify(d2GatedRoutes));

  const d2r = freshAbility(mod, FIXTURE_FILESDIR);
  d2r['frozenEndpointExclusions'] = derived;
  resetLog();
  const d2rSubmitted = await driveRecreateCapture(d2r, netcfgRecreate16());
  const d2rInjected = injectedOf(legacyRecreateTunnelConfig(derived, netcfgRecreate16()));
  check('D2 recreate leg: routes carry the network entry and the config IS the pure composition',
    d2rSubmitted.length === 1 &&
    deepEq(d2rSubmitted[0], platformOf(d2rInjected['config'])) &&
    ((d2rSubmitted[0]['routes'] as Array<Record<string, unknown>>).filter(
      (r: Record<string, unknown>): boolean =>
        ((r['destination'] as Record<string, unknown>)['address'] as Record<string, unknown>)['address']
        === '100.108.0.0').length === 1),
    JSON.stringify(d2rSubmitted[0]));
  check('D2 recreate leg: the marker says leg=recreate and the marker counts match the submission',
    lastLine(TUNNEL_SUBNET_ROUTE) === 'VPN_TUNNEL_SUBNET_ROUTE|requestId=' + REQUEST_ID +
    '|addr=100.108.144.165|prefix=16|network=100.108.0.0|leg=recreate|result=added|reason=none' &&
    fieldOf(lastLine('VPN_EXCLUSION_GENERATION|'), 'routes') ===
    String((d2rSubmitted[0]['routes'] as unknown[]).length) &&
    linesWith('VPN_ROUTE_ENTRY|').length === (d2rSubmitted[0]['routes'] as unknown[]).length,
    lastLine(TUNNEL_SUBNET_ROUTE));
  const d2r32 = freshAbility(mod, FIXTURE_FILESDIR);
  d2r32['frozenEndpointExclusions'] = derived;
  resetLog();
  const d2r32Submitted = await driveRecreateCapture(d2r32, netcfgJson());
  check('D2 recreate leg with a /32 tunnel address: skipped with reason=prefix-32 and the ' +
    'submitted config stays the legacy one',
    d2r32Submitted.length === 1 &&
    deepEq(d2r32Submitted[0], platformOf(legacyRecreateTunnelConfig(derived, netcfgJson()))) &&
    lastLine(TUNNEL_SUBNET_ROUTE) === 'VPN_TUNNEL_SUBNET_ROUTE|requestId=' + REQUEST_ID +
    '|addr=10.99.0.1|prefix=32|network=10.99.0.1|leg=recreate|result=skipped|reason=prefix-32',
    lastLine(TUNNEL_SUBNET_ROUTE));

  // Skip/duplicate paths through the REAL private call site (initial leg
  // shape): the injected entry must never be a guess.
  const d2skip = freshAbility(mod, FIXTURE_FILESDIR);
  const d2SkipBase = legacyInitialTunnelConfig(d2, NETCFG_0021);
  resetLog();
  const d2SkipOut = (d2skip['injectTunnelSubnetRoute'] as
    (c: unknown, leg: string) => Record<string, unknown>)(
    Object.assign({}, d2SkipBase, { tunnelAddress: 'mgmt.test' }), 'initial');
  check('D2 a non-IPv4 tunnel address adds nothing and is logged as invalid-address',
    ((d2SkipOut['routes'] ?? []) as unknown[]).length ===
    (d2SkipBase['routes'] as unknown[]).length &&
    lastLine(TUNNEL_SUBNET_ROUTE) === 'VPN_TUNNEL_SUBNET_ROUTE|requestId=' + REQUEST_ID +
    '|addr=mgmt.test|prefix=16|network=|leg=initial|result=skipped|reason=invalid-address',
    lastLine(TUNNEL_SUBNET_ROUTE));
  resetLog();
  (d2skip['injectTunnelSubnetRoute'] as (c: unknown, leg: string) => Record<string, unknown>)(
    Object.assign({}, d2SkipBase, { tunnelAddress: '2001:db8::1' }), 'initial');
  check('D2 an IPv6 tunnel address is refused as not-ipv4 (never parsed as a subnet)',
    lastLine(TUNNEL_SUBNET_ROUTE).indexOf(
      '|network=|leg=initial|result=skipped|reason=not-ipv4') > 0,
    lastLine(TUNNEL_SUBNET_ROUTE));
  resetLog();
  const d2DupOut = (d2skip['injectTunnelSubnetRoute'] as
    (c: unknown, leg: string) => Record<string, unknown>)(
    Object.assign({}, d2SkipBase, { tunnelAddress: '10.10.10.1', tunnelPrefixLength: 24 }),
    'initial');
  check('D2 an already-present destination+prefix is not duplicated (reason=duplicate)',
    ((d2DupOut['routes'] ?? []) as unknown[]).length ===
    (d2SkipBase['routes'] as unknown[]).length &&
    lastLine(TUNNEL_SUBNET_ROUTE).indexOf(
      '|addr=10.10.10.1|prefix=24|network=10.10.10.0|leg=initial|result=skipped|reason=duplicate') > 0,
    lastLine(TUNNEL_SUBNET_ROUTE));

  // ---- D3③: S2 on — blockedApplications = the self bundle, BOTH legs ------
  const d3 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({ diag_blocked_self: true }));
  resetLog();
  const d3Submitted = await driveInitialLeg(d3, NETCFG_0021);
  const d3Legacy = legacyInitialTunnelConfig(d3, NETCFG_0021);
  check('D3 initial leg: blockedApplications is EXACTLY [cn.alfadb.netbird], trustedApplications absent, ' +
    'routes = the default composition (legacy + the tunnel-subnet entry)',
    d3Submitted.length === 1 &&
    deepEq(d3Submitted[0]['blockedApplications'], [SELF_BUNDLE]) &&
    !('trustedApplications' in (d3Submitted[0] ?? {})) &&
    deepEq(Object.keys(d3Submitted[0]), ['addresses', 'routes', 'dnsAddresses', 'mtu',
      'blockedApplications']) &&
    deepEq(d3Submitted[0]['routes'],
      platformOf(injectedOf(d3Legacy)['config'])['routes']),
    JSON.stringify(d3Submitted[0]));
  check('D3 initial leg: the S2 marker is the fixed literal',
    lastLine(DIAG_BLOCKED_APPS) === 'N2H_DIAG_BLOCKED_APPS|requestId=' + REQUEST_ID +
    '|bundle=' + SELF_BUNDLE + '|count=1|leg=initial|diag=true|is_evidence=false',
    lastLine(DIAG_BLOCKED_APPS));
  const d3r = freshAbility(mod, FIXTURE_FILESDIR);
  d3r['frozenEndpointExclusions'] = derived;
  d3r['diagBlockedSelf'] = true;
  resetLog();
  const d3rSubmitted = await driveRecreateCapture(d3r, netcfgJson());
  check('D3 recreate leg: blockedApplications is exactly the self bundle and the marker says recreate',
    d3rSubmitted.length === 1 &&
    deepEq(d3rSubmitted[0]['blockedApplications'], [SELF_BUNDLE]) &&
    !('trustedApplications' in (d3rSubmitted[0] ?? {})) &&
    deepEq(d3rSubmitted[0], platformOf(legacyRecreateTunnelConfig(derived, netcfgJson()),
      [SELF_BUNDLE])) &&
    lastLine(DIAG_BLOCKED_APPS) === 'N2H_DIAG_BLOCKED_APPS|requestId=' + REQUEST_ID +
    '|bundle=' + SELF_BUNDLE + '|count=1|leg=recreate|diag=true|is_evidence=false',
    JSON.stringify(d3rSubmitted[0]));

  // ---- D4④: S3 — one real protectProcessNet() call, four outcomes ---------
  async function settleWith(ability: Record<string, unknown>, fd: number, gen: number,
    open: boolean): Promise<Record<string, unknown>> {
    G['__NB_FD_STATUS'] = open ? { open: true, errno: 0 } : { open: false, errno: 9 };
    return await (ability['settleCreate'] as
      (f: number, g: number) => Promise<Record<string, unknown>>)(fd, gen);
  }

  const d4off = freshAbility(mod, FIXTURE_FILESDIR);
  d4off['connection'] = new FakeConnection();
  G['__NB_PROCESS_PROTECT_MODE'] = 'resolved';
  G['__NB_PROCESS_PROTECT_CALLS'] = 0;
  resetLog();
  await settleWith(d4off, 42, 0, true);
  await drainProbe();
  check('D4 S3 off: an accepted create() calls protectProcessNet ZERO times and logs no PROCESS_NET line',
    G['__NB_PROCESS_PROTECT_CALLS'] === 0 && linesWith(PROCESS_NET).length === 0,
    String(G['__NB_PROCESS_PROTECT_CALLS']));

  const d4on = freshAbility(mod, FIXTURE_FILESDIR);
  d4on['diagProtectProcessNet'] = true;
  d4on['connection'] = new FakeConnection();
  G['__NB_PROCESS_PROTECT_MODE'] = 'resolved';
  G['__NB_PROCESS_PROTECT_CALLS'] = 0;
  resetLog();
  const d4First = await settleWith(d4on, 42, 0, true);
  await drainProbe();
  check('D4 an accepted create() calls protectProcessNet EXACTLY once, after the acceptance anchor, ' +
    'and the resolved line carries the fixed s3 tail',
    G['__NB_PROCESS_PROTECT_CALLS'] === 1 && d4First['accepted'] === true &&
    linesWith(PROCESS_NET).length === 1 &&
    /^VPN_PROTECT_PROCESS_NET\|requestId=req-i3a\|result=resolved\|elapsedMs=\d+\|switch=s3\|diag=true\|is_evidence=false$/
      .test(lastLine(PROCESS_NET)) &&
    logs().join('\n').indexOf('VPN_CREATE_RESOLVED|requestId=req-i3a|fd=42|accepted=true') <
    logs().join('\n').indexOf('VPN_PROTECT_PROCESS_NET|'),
    lastLine(PROCESS_NET));
  resetLog();
  await settleWith(d4on, 43, 1, true);
  await drainProbe();
  check('D4 the latch holds: a SECOND accepted settle (recreate generation) calls nothing again',
    G['__NB_PROCESS_PROTECT_CALLS'] === 1 && linesWith(PROCESS_NET).length === 0,
    String(G['__NB_PROCESS_PROTECT_CALLS']));

  const d4rej = freshAbility(mod, FIXTURE_FILESDIR);
  d4rej['diagProtectProcessNet'] = true;
  d4rej['connection'] = new FakeConnection();
  G['__NB_PROCESS_PROTECT_CALLS'] = 0;
  resetLog();
  const d4NotAccepted = await settleWith(d4rej, 7, 0, false);
  await drainProbe();
  check('D4 a create() that is NOT accepted never calls protectProcessNet (no call, no line)',
    d4NotAccepted['accepted'] === false && G['__NB_PROCESS_PROTECT_CALLS'] === 0 &&
    linesWith(PROCESS_NET).length === 0, JSON.stringify(d4NotAccepted));

  const d4dev = freshAbility(mod, FIXTURE_FILESDIR);
  d4dev['diagProtectProcessNet'] = true;
  d4dev['protectDeviationRouteExclusion'] = true; // the route-exclusion deviation
  d4dev['connection'] = new FakeConnection();
  G['__NB_PROCESS_PROTECT_MODE'] = 'resolved';
  G['__NB_PROCESS_PROTECT_CALLS'] = 0;
  resetLog();
  await settleWith(d4dev, 42, 0, true);
  await drainProbe();
  check('D4 the protect-deviation short-circuit does NOT suppress the real S3 platform call',
    G['__NB_PROCESS_PROTECT_CALLS'] === 1 && linesWith(PROCESS_NET).length === 1,
    String(G['__NB_PROCESS_PROTECT_CALLS']));

  // The four outcome literals, driven through the REAL wrapper.
  async function s3Outcome(mode: string): Promise<Record<string, unknown>> {
    G['__NB_PROCESS_PROTECT_MODE'] = mode;
    const ability = freshAbility(mod, FIXTURE_FILESDIR);
    resetLog();
    const ok = await (ability['protectProcessNetWithBox'] as
      (c: unknown) => Promise<boolean>)(new FakeConnection());
    return { ok: ok, line: lastLine(PROCESS_NET), count: linesWith(PROCESS_NET).length };
  }
  const d4resolved = await s3Outcome('resolved');
  check('D4 outcome resolved: returns true, ONE line, fixed literal',
    d4resolved['ok'] === true && d4resolved['count'] === 1 &&
    /^VPN_PROTECT_PROCESS_NET\|requestId=req-i3a\|result=resolved\|elapsedMs=\d+\|switch=s3\|diag=true\|is_evidence=false$/
      .test(String(d4resolved['line'])), String(d4resolved['line']));
  const d4rejected = await s3Outcome('rejected');
  check('D4 outcome rejected: returns false, ONE line with the error summary and the s3 tail',
    d4rejected['ok'] === false && d4rejected['count'] === 1 &&
    /^VPN_PROTECT_PROCESS_NET\|requestId=req-i3a\|result=rejected\|elapsedMs=\d+\|summary=code=none,name=Error,message=process-protect-rejected\|switch=s3\|diag=true\|is_evidence=false$/
      .test(String(d4rejected['line'])), String(d4rejected['line']));
  const d4throw = await s3Outcome('throw');
  check('D4 outcome throw (SYNCHRONOUS throw): returns false, ONE line, fixed literal',
    d4throw['ok'] === false && d4throw['count'] === 1 &&
    /^VPN_PROTECT_PROCESS_NET\|requestId=req-i3a\|result=throw\|summary=code=none,name=Error,message=process-protect-throw\|switch=s3\|diag=true\|is_evidence=false$/
      .test(String(d4throw['line'])), String(d4throw['line']));
  const d4never = await s3Outcome('never');
  // The 5 s wait of THIS case also covers the settled-box honesty: the box
  // timers of the resolved / rejected / throw calls above are CLEARED when
  // their platform call settles, so none of them can reach the log at all and
  // the single line below can only be the real timeout.
  check('D4 outcome never-settles (the device-observed wedge): the bounded box answers ' +
    'result=timeout after boxMs=5000, the wrapper reports false, and the settled calls ' +
    'above leave no stale timeout line behind',
    d4never['ok'] === false && d4never['count'] === 1 &&
    String(d4never['line']) === 'VPN_PROTECT_PROCESS_NET|requestId=' + REQUEST_ID +
    '|result=timeout|boxMs=5000|switch=s3|diag=true|is_evidence=false',
    String(d4never['line']) + ' count=' + String(d4never['count']));

  // ---- D4④b: the bounded box timer is CANCELLED once the call settles -----
  // Found 2026-09-25: both wrappers armed their 5 s box with setTimeout and
  // never cleared the handle, so a SUCCESSFUL protect() printed a second,
  // false VPN_PROTECT_TIMEOUT line (stillPending=true) five seconds later —
  // indistinguishable from a real wedge in the device evidence.
  // protectProcessNetWithBox was saved by its `raceSettled` log gate;
  // protectSocketWithBox had no gate and printed the lie. The assertions
  // below pin BOTH wrappers: a settled call leaves NO timeout line (checked
  // after a FULL box window, carried past its own deadline by the next,
  // never-settling call) while a never-settling call still reports exactly
  // ONE timeout line after the unchanged 5 s box.
  const PROTECT_TIMEOUT = 'VPN_PROTECT_TIMEOUT|';
  // The elapsed witness below only proves that the 5 s deadline PASSED inside
  // this window (a timer that survived cancellation would have fired in it) —
  // it is never an assertion about exact milliseconds. The box fires 5000 ms
  // after it is armed, and the settled call before it resolves in microtasks,
  // so `Date.now()` at expiry can still be the SAME millisecond the witness
  // was sampled in: the difference is then exactly 5000 and a strict `>` fails
  // on the boundary. The comparison carries a small tolerance instead; the
  // load-bearing assertions stay the line sets (NO timeout line for the
  // settled call, EXACTLY one for the never-settling one).
  const D4B_BOX_MS = 5000;
  const D4B_BOX_TOLERANCE_MS = 50;
  function processNetTimeouts(): string[] {
    return linesWith(PROCESS_NET)
      .filter((l: string): boolean => l.indexOf('|result=timeout|') >= 0);
  }
  const d4b = freshAbility(mod, FIXTURE_FILESDIR);
  const d4bConnection = new FakeConnection();
  d4b['connection'] = d4bConnection;
  G['__NB_PROTECT_MODE'] = 'resolved';
  G['__NB_PROTECTED'] = 0;
  resetLog();
  const d4bResolvedAt = Date.now();
  const d4bOk = await (d4b['protectSocketWithBox'] as
    (c: unknown, fd: number) => Promise<boolean>)(d4bConnection, 42);
  check('D4b protect resolved: the real platform call DID happen (counted once), the wrapper ' +
    'returns true with ONE SETTLED line and NO timeout line',
    d4bOk === true && G['__NB_PROTECTED'] === 1 &&
    linesWith('VPN_PROTECT_SETTLED|').length === 1 &&
    linesWith(PROTECT_TIMEOUT).length === 0,
    JSON.stringify({ ok: d4bOk, calls: G['__NB_PROTECTED'],
      timeouts: linesWith(PROTECT_TIMEOUT) }));
  G['__NB_PROTECT_MODE'] = 'never';
  const d4bNeverOk = await (d4b['protectSocketWithBox'] as
    (c: unknown, fd: number) => Promise<boolean>)(d4bConnection, 43);
  // Restore the mode BEFORE asserting: the D5/D6/D7 groups below drive the
  // REAL initial leg, whose fail-closed protect gate must not inherit the
  // wedge shape.
  G['__NB_PROTECT_MODE'] = 'resolved';
  check('D4b protect never settles: the WHOLE box still answers false with exactly ONE ' +
    'stillPending=true timeout line (fd=43) after boxMs=5000, and the resolved fd=42 above — ' +
    'its 5 s deadline having passed inside this window — left no line at all',
    d4bNeverOk === false && Date.now() - d4bResolvedAt >= D4B_BOX_MS - D4B_BOX_TOLERANCE_MS &&
    deepEq(linesWith(PROTECT_TIMEOUT), [
      'VPN_PROTECT_TIMEOUT|requestId=' + REQUEST_ID + '|fd=43|boxMs=5000|stillPending=true'
    ]), JSON.stringify(linesWith(PROTECT_TIMEOUT)));
  const d4c = freshAbility(mod, FIXTURE_FILESDIR);
  G['__NB_PROCESS_PROTECT_MODE'] = 'resolved';
  resetLog();
  const d4cResolvedAt = Date.now();
  const d4cOk = await (d4c['protectProcessNetWithBox'] as
    (c: unknown) => Promise<boolean>)(new FakeConnection());
  check('D4b protectProcessNet resolved: returns true with ONE result=resolved line and NO ' +
    'timeout line',
    d4cOk === true && linesWith(PROCESS_NET).length === 1 &&
    /^VPN_PROTECT_PROCESS_NET\|requestId=req-i3a\|result=resolved\|elapsedMs=\d+\|switch=s3\|diag=true\|is_evidence=false$/
      .test(lastLine(PROCESS_NET)) &&
    processNetTimeouts().length === 0,
    JSON.stringify({ ok: d4cOk, lines: linesWith(PROCESS_NET) }));
  G['__NB_PROCESS_PROTECT_MODE'] = 'never';
  const d4cNeverOk = await (d4c['protectProcessNetWithBox'] as
    (c: unknown) => Promise<boolean>)(new FakeConnection());
  G['__NB_PROCESS_PROTECT_MODE'] = 'resolved';
  check('D4b protectProcessNet never settles: the WHOLE box still answers false with exactly ' +
    'ONE result=timeout line after boxMs=5000, and the resolved call above — its 5 s deadline ' +
    'having passed inside this window — left no stale line',
    d4cNeverOk === false && Date.now() - d4cResolvedAt >= D4B_BOX_MS - D4B_BOX_TOLERANCE_MS &&
    deepEq(processNetTimeouts(), [
      'VPN_PROTECT_PROCESS_NET|requestId=' + REQUEST_ID +
      '|result=timeout|boxMs=5000|switch=s3|diag=true|is_evidence=false'
    ]), JSON.stringify(processNetTimeouts()));

  // ---- D5⑤: S4 on — the probe gets NO probe_net_id -----------------------
  const d5 = freshAbility(mod, FIXTURE_FILESDIR);
  d5['n2hAcceptedExclusionGeneration'] = 0;
  setArmFile(true, undefined, JSON.stringify({ diag_posctl_unbound: true, posctl_port: 41000 }));
  (d5['readDiagSwitches'] as () => void)();
  seedNetCtx({ __NB_NET_ALL_IDS: [147, 101], __NB_NET_PROPS_BY_ID: { 147: { interfaceName: 'vpn-tun' } } });
  resetLog();
  tickArm(d5, WG_READY);
  await drainProbe();
  check('D5 S4 on: probe_net_id is withheld even though a vpn-tun handle WAS resolvable, ' +
    'the UNBOUND marker is printed instead and the UNAVAILABLE marker is NOT',
    d5['diagPosctlUnbound'] === true && armOptsJson()['probe_net_id'] === undefined &&
    lastLine(DIAG_UNBOUND) === 'N2H_DIAG_POSCTL_UNBOUND|requestId=' + REQUEST_ID +
    '|diag=true|is_evidence=false' &&
    linesWith(DIAG_UNBOUND).length === 1 &&
    linesWith(PROBE_NET_MARKER).length === 0 &&
    armOptsJson()['posctl_port'] === 41000,
    JSON.stringify({ opts: armOptsJson(), unbound: linesWith(DIAG_UNBOUND) }));

  // ---- D6⑥: a present but WRONG-TYPED key is OFF and logged ---------------
  const d6 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({
    diag_tunnel_route: 'true', diag_blocked_self: 1,
    diag_protect_process_net: null, diag_posctl_unbound: false
  }));
  resetLog();
  (d6['readDiagSwitches'] as () => void)();
  const d6Ignored = linesWith(DIAG_IGNORED);
  check('D6 each wrong-typed key prints ONE ignored line naming its own key, reason=not-boolean, ' +
    'in ticket order — and the RETIRED "diag_tunnel_route" key prints NOTHING',
    deepEq(d6Ignored, [
      'N2H_DIAG_OPTION_IGNORED|key=diag_blocked_self|reason=not-boolean|diag=true|is_evidence=false',
      'N2H_DIAG_OPTION_IGNORED|key=diag_protect_process_net|reason=not-boolean|diag=true|is_evidence=false'
    ]), JSON.stringify(d6Ignored));
  check('D6 the wrong-typed keys are OFF — "true" the STRING / 1 / null never enable a switch, ' +
    'an explicit false is a silent typed OFF, and the retired S1 key leaves no field at all',
    d6['diagTunnelRoute'] === undefined && d6['diagBlockedSelf'] === false &&
    d6['diagProtectProcessNet'] === false && d6['diagPosctlUnbound'] === false,
    JSON.stringify({ a: d6['diagTunnelRoute'], b: d6['diagBlockedSelf'],
      c: d6['diagProtectProcessNet'], d: d6['diagPosctlUnbound'] }));
  const d6Drive = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({ diag_tunnel_route: 'true', diag_blocked_self: 1 }));
  resetLog();
  const d6Submitted = await driveInitialLeg(d6Drive, NETCFG_0021);
  check('D6 a wrong-typed ticket changes NOTHING beyond the default: the submission is the ' +
    'legacy composition PLUS the default tunnel-subnet route, no blockedApplications, and the ' +
    'only N2H_DIAG_ line is the one ignored-key line',
    d6Submitted.length === 1 &&
    deepEq(d6Submitted[0],
      platformOf(injectedOf(legacyInitialTunnelConfig(d6Drive, NETCFG_0021))['config'])) &&
    !('blockedApplications' in (d6Submitted[0] ?? {})) &&
    deepEq(diagLines(), [
      'N2H_DIAG_OPTION_IGNORED|key=diag_blocked_self|reason=not-boolean|diag=true|is_evidence=false'
    ]), JSON.stringify(diagLines()));
  const d6Silent = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({ posctl_port: 41000 }));
  resetLog();
  (d6Silent['readDiagSwitches'] as () => void)();
  const d6OtherKeysSilent = diagLines().length === 0;
  setSwitchTicket('not-json-at-all');
  resetLog();
  (d6Silent['readDiagSwitches'] as () => void)();
  const d6MalformedSilent = diagLines().length === 0;
  setSwitchTicket('null');
  resetLog();
  (d6Silent['readDiagSwitches'] as () => void)();
  const d6NullSilent = diagLines().length === 0 && d6Silent['diagTunnelRoute'] === undefined;
  setSwitchTicket(undefined);
  resetLog();
  (d6Silent['readDiagSwitches'] as () => void)();
  check('D6 absence, unrelated keys, malformed JSON and JSON null are ALL silent and off',
    d6OtherKeysSilent && d6MalformedSilent && d6NullSilent && diagLines().length === 0 &&
    d6Silent['diagTunnelRoute'] === undefined && d6Silent['diagBlockedSelf'] === false &&
    d6Silent['diagProtectProcessNet'] === false && d6Silent['diagPosctlUnbound'] === false,
    JSON.stringify(diagLines()));

  // ---- D7⑦: the tunnel-subnet pure function (unit) ------------------------
  function tunCfg(address: string, prefix: number,
    routes: unknown[]): Record<string, unknown> {
    return { tunnelAddress: address, tunnelPrefixLength: prefix, interfaceName: 'vpn-tun',
      routes: routes, dnsServers: ['8.8.8.8'], mtu: 1400, peerEndpoint: '192.0.2.10:51820' };
  }
  function injectAt(address: string, prefix: number,
    routes: unknown[] = []): Record<string, unknown> {
    return (cfg['withTunnelSubnetRoute'] as
      (c: unknown) => Record<string, unknown>)(tunCfg(address, prefix, routes));
  }
  const d7Input = tunCfg('100.108.144.165', 16, []);
  const d7P16 = (cfg['withTunnelSubnetRoute'] as
    (c: unknown) => Record<string, unknown>)(d7Input);
  check('D7 /16: 100.108.144.165/16 → 100.108.0.0/16, appended in the managed shape',
    d7P16['added'] === true && d7P16['network'] === '100.108.0.0' && d7P16['reason'] === 'none' &&
    deepEq((d7P16['config'] as Record<string, unknown>)['routes'], [{
      destination: '100.108.0.0', prefixLength: 16, gateway: '100.108.144.165',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: false
    }]), JSON.stringify(d7P16));
  check('D7 the input config is NOT mutated (pure function)',
    (d7Input['routes'] as unknown[]).length === 0, JSON.stringify(d7Input));
  check('D7 /24 and /8 network addresses are computed on the octet boundary',
    injectAt('100.108.144.165', 24)['network'] === '100.108.144.0' &&
    injectAt('10.20.30.40', 8)['network'] === '10.0.0.0',
    String(injectAt('100.108.144.165', 24)['network']));
  const d7P32 = injectAt('100.108.144.165', 32);
  check('D7 /32 is NEVER added (a host route, not the subnet) and is logged as prefix-32',
    d7P32['added'] === false && d7P32['reason'] === 'prefix-32' &&
    d7P32['network'] === '100.108.144.165' &&
    deepEq((d7P32['config'] as Record<string, unknown>)['routes'], []), JSON.stringify(d7P32));
  check('D7 invalid addresses are refused as invalid-address (never guessed)',
    injectAt('mgmt.test', 16)['reason'] === 'invalid-address' &&
    injectAt('mgmt.test', 16)['network'] === '' &&
    injectAt('1.2.3.256', 16)['reason'] === 'invalid-address' &&
    injectAt('', 16)['reason'] === 'invalid-address' &&
    injectAt('10.1.2.3.4', 16)['reason'] === 'invalid-address',
    String(injectAt('1.2.3.256', 16)['reason']));
  check('D7 an IPv6 tunnel address (and the IPv4:port dial shape) is refused as not-ipv4',
    injectAt('2001:db8::1', 64)['reason'] === 'not-ipv4' &&
    injectAt('2001:db8::1', 64)['network'] === '' &&
    injectAt('203.0.113.7:443', 16)['reason'] === 'not-ipv4',
    String(injectAt('2001:db8::1', 64)['reason']));
  check('D7 prefix 0 / 33 / NaN is out of range for a managed non-default route',
    injectAt('100.108.144.165', 0)['reason'] === 'prefix-out-of-range' &&
    injectAt('100.108.144.165', 33)['reason'] === 'prefix-out-of-range' &&
    injectAt('100.108.144.165', Number.NaN)['reason'] === 'prefix-out-of-range',
    String(injectAt('100.108.144.165', 0)['reason']));
  const d7DupRoutes = [{ destination: '100.108.0.0', prefixLength: 16, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: false }];
  const d7Dup = injectAt('100.108.144.165', 16, d7DupRoutes);
  check('D7 an existing same destination+prefix is not duplicated',
    d7Dup['added'] === false && d7Dup['reason'] === 'duplicate' &&
    (d7Dup['config'] as Record<string, unknown>)['routes'] === d7DupRoutes,
    JSON.stringify(d7Dup));
  const d7Gated = (cfg['gateDefaultRoutes'] as
    (c: unknown, a: boolean) => Record<string, unknown>)(d7P16['config'], false);
  check('D7 gateDefaultRoutes(false) keeps the tunnel-subnet entry (it is never an ' +
    'isDefaultRoute entry)',
    ((d7Gated['routes'] ?? []) as unknown[]).length === 1,
    JSON.stringify(d7Gated));

  // ---------------- CR (2026-09-26): the S6 capture route -------------------
  // The protect-bypass plan's capture face (n2h-kit/protect-bypass-plan-
  // 20260926.md §3.1 + §6 item 1②): with the DEFAULT-OFF arm-ticket switch
  // `diag_capture_route` ON and a usable `protect_probe.target`, the shell
  // appends ONE explicit NON-excluded `<target>/32` route on BOTH legs, in the
  // same placement as the tunnel-subnet entry. Driven end-to-end through the
  // REAL startConnectorThenCreate / performControlledRecreate, so "the
  // submitted VpnConfig" is the FakeConnection's captured platform document.
  // CR1 pins the wiring; CR2 the default (off, byte-identical, silent); CR3/CR4
  // the added /32 on the initial and recreate legs; CR5 the excluded target
  // (the anti-masking refusal); CR6 the pure helper's strict target acceptance;
  // CR7 the switch on with no usable target.
  const CAPTURE_LINE = 'N2H_DIAG_CAPTURE_ROUTE|';

  function captureLines(): string[] {
    return linesWith(CAPTURE_LINE);
  }

  function destinationAddresses(platformConfig: Record<string, unknown>): string[] {
    return (((platformConfig ?? {})['routes'] ?? []) as Array<Record<string, unknown>>).map(
      (r: Record<string, unknown>): string => String(
        ((r['destination'] as Record<string, unknown>)['address'] as Record<string, unknown>)['address']));
  }

  function routeRowsWith(config: Record<string, unknown>,
    ip: string): Array<Record<string, unknown>> {
    return ((config['routes'] ?? []) as Array<Record<string, unknown>>).filter(
      (r: Record<string, unknown>): boolean =>
        ((r['destination'] as Record<string, unknown>)['address'] as Record<string, unknown>)['address']
        === ip);
  }

  // The managed-route shape the capture row must have: vpn-tun, /32, the
  // tunnel address as gateway, non-default, and — the load-bearing half —
  // NO isExcludedRoute (the platform mapper omits the field for a non-excluded
  // route, so its presence would mean the capture face silently became a
  // masking exclusion).
  function captureRow(ip: string, gateway: string): Record<string, unknown> {
    return {
      interface: 'vpn-tun',
      destination: { address: { address: ip, family: 1 }, prefixLength: 32 },
      gateway: { address: gateway, family: 1 },
      hasGateway: true,
      isDefaultRoute: false
    };
  }

  // CR1: wiring pins on the byte-identical source.
  const capKeyAt = extSource.indexOf(
    "const N2H_DIAG_CAPTURE_ROUTE_KEY: string = 'diag_capture_route';");
  const capResetAt = extSource.indexOf('this.diagCaptureRoute = false;');
  const capReadAt = extSource.indexOf(
    'this.diagCaptureRoute = this.readDiagSwitch(doc, N2H_DIAG_CAPTURE_ROUTE_KEY);');
  const capTargetAt = extSource.indexOf(
    'this.diagCaptureTarget = this.readProtectProbeTarget(doc);');
  const capInitialAt = extSource.indexOf("this.injectCaptureRoute(merged, 'initial')");
  const capRecreateAt = extSource.indexOf("this.injectCaptureRoute(merged, 'recreate')");
  const runArmBodyMatch = /private async runN2hArm\([\s\S]*?\n  \}/.exec(extSource);
  const runArmBodyAt = runArmBodyMatch === null ? -1 : (runArmBodyMatch.index ?? -1);
  const runArmBody = runArmBodyMatch === null ? '' : runArmBodyMatch[0];
  check('CR1 the switch + its target are read ONCE in readDiagSwitches (key read via the shared ' +
    'readDiagSwitch, target from the ticket\'s protect_probe object), both re-defaulted first',
    capKeyAt > 0 && capResetAt > 0 && capReadAt > capResetAt && capTargetAt > capReadAt,
    JSON.stringify({ capKeyAt, capResetAt, capReadAt, capTargetAt }));
  check('CR1 the capture route has exactly TWO call sites (initial + recreate), each behind the ' +
    'switch gate, in the SAME placement as the tunnel-subnet entry',
    extSource.split('this.injectCaptureRoute(merged,').length === 3 &&
    extSource.split('if (this.diagCaptureRoute) {').length === 3 &&
    capInitialAt > subnetInitialAt && capInitialAt < diagLogInitialAt &&
    capRecreateAt > subnetRecreateAt && capRecreateAt < diagLogRecreateAt &&
    extSource.indexOf(
      "'N2H_DIAG_CAPTURE_ROUTE|requestId=%{public}s|target=%{public}s|leg=%{public}s|' +") > 0 &&
    extSource.indexOf(
      "'result=%{public}s|reason=%{public}s|diag=true|is_evidence=false',") > 0,
    JSON.stringify({ capInitialAt, subnetInitialAt, diagLogInitialAt, capRecreateAt,
      subnetRecreateAt, diagLogRecreateAt }));

  // CR2: the default. A ticket that carries the target but NOT the switch
  // (an explicit boolean false here — the deliberate-disable shape) must leave
  // the submission byte-identical and print nothing.
  const cr2 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({
    diag_capture_route: false,
    protect_probe: { target: '203.0.113.9:53', count: 2, mode: 'none' }
  }));
  resetLog();
  const cr2Submitted = await driveInitialLeg(cr2, NETCFG_0021);
  const cr2Default = platformOf(injectedOf(legacyInitialTunnelConfig(cr2, NETCFG_0021))['config']);
  check('CR2 switch OFF (explicit false) with a target present: the submitted VpnConfig is the ' +
    'default composition BYTE-FOR-BYTE, no capture /32 exists and NO capture line is printed',
    cr2Submitted.length === 1 && deepEq(cr2Submitted[0], cr2Default) &&
    destinationAddresses(cr2Submitted[0]).indexOf('203.0.113.9') < 0 &&
    cr2['diagCaptureRoute'] === false && cr2['diagCaptureTarget'] === '203.0.113.9:53' &&
    captureLines().length === 0,
    JSON.stringify({ submitted: cr2Submitted[0], capture: captureLines() }));

  // CR3: the switch ON — the initial leg.
  const cr3 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({
    diag_capture_route: true,
    protect_probe: { target: '203.0.113.9:53', count: 2, mode: 'protect' }
  }));
  resetLog();
  const cr3Submitted = await driveInitialLeg(cr3, NETCFG_0021);
  const cr3Base = platformOf(injectedOf(legacyInitialTunnelConfig(cr3, NETCFG_0021))['config']);
  const cr3Routes = (cr3Submitted[0] ?? {})['routes'] as Array<Record<string, unknown>>;
  const cr3Cap = routeRowsWith(cr3Submitted[0] ?? {}, '203.0.113.9');
  check('CR3 initial leg: exactly ONE extra route, appended AFTER the tunnel-subnet entry, and it ' +
    'is the managed /32 capture shape (vpn-tun, gateway = the tunnel address, non-default, and ' +
    'isExcludedRoute ABSENT)',
    cr3Submitted.length === 1 && cr3Routes.length ===
    ((cr3Base['routes'] ?? []) as unknown[]).length + 1 &&
    deepEq(cr3Routes.slice(0, cr3Routes.length - 1), cr3Base['routes']) &&
    cr3Cap.length === 1 && deepEq(cr3Cap[0], captureRow('203.0.113.9', '100.108.144.165')) &&
    !('isExcludedRoute' in cr3Cap[0]),
    JSON.stringify({ routes: cr3Routes, base: cr3Base['routes'] }));
  check('CR3 initial leg: the capture line is the fixed literal (leg=initial, added/none, ' +
    'diag tail) and exactly one is printed',
    deepEq(captureLines(), ['N2H_DIAG_CAPTURE_ROUTE|requestId=' + REQUEST_ID +
      '|target=203.0.113.9|leg=initial|result=added|reason=none|diag=true|is_evidence=false']),
    JSON.stringify(captureLines()));
  check('CR3 initial leg: the generation marker, the route rows and the fail-closed validation ' +
    'gate all cover the capture row (it is announced and accepted, not smuggled in)',
    fieldOf(lastLine('VPN_EXCLUSION_GENERATION|'), 'routes') === String(cr3Routes.length) &&
    linesWith('VPN_ROUTE_ENTRY|').length === cr3Routes.length &&
    linesWith('VPN_ROUTE_VIOLATION|').length === 0,
    lastLine('VPN_EXCLUSION_GENERATION|') + ' rows=' +
    String(linesWith('VPN_ROUTE_ENTRY|').length));
  check('CR3 the capture-route switch is never read on the ARM path — it can only ever gate the ' +
    'route injection, so it has no path into the opts document',
    runArmBodyAt > 0 && runArmBody.indexOf('diagCaptureRoute') < 0 &&
    runArmBody.indexOf('N2H_DIAG_CAPTURE_ROUTE_KEY') < 0,
    JSON.stringify({ runArmBodyAt, len: runArmBody.length }));

  // CR4: the switch ON — the recreate leg (the platform VpnConfig is fully
  // replaced, so the capture entry must be re-declared there too).
  const cr4 = freshAbility(mod, FIXTURE_FILESDIR);
  cr4['frozenEndpointExclusions'] = derived;
  cr4['diagCaptureRoute'] = true;
  cr4['diagCaptureTarget'] = '203.0.113.44:53';
  resetLog();
  const cr4Submitted = await driveRecreateCapture(cr4, netcfgRecreate16());
  const cr4Base = platformOf(
    injectedOf(legacyRecreateTunnelConfig(derived, netcfgRecreate16()))['config']);
  const cr4Routes = (cr4Submitted[0] ?? {})['routes'] as Array<Record<string, unknown>>;
  const cr4Cap = routeRowsWith(cr4Submitted[0] ?? {}, '203.0.113.44');
  check('CR4 recreate leg: the capture /32 is re-declared (managed shape, after the tunnel-subnet ' +
    'entry, non-excluded) and the frozen endpoint exclusions are untouched',
    cr4Submitted.length === 1 && cr4Routes.length ===
    ((cr4Base['routes'] ?? []) as unknown[]).length + 1 &&
    deepEq(cr4Routes.slice(0, cr4Routes.length - 1), cr4Base['routes']) &&
    cr4Cap.length === 1 && deepEq(cr4Cap[0], captureRow('203.0.113.44', '100.108.144.165')) &&
    !('isExcludedRoute' in cr4Cap[0]) &&
    destinationAddresses(cr4Submitted[0]).filter(
      (a: string): boolean => a === '203.0.113.7').length === 1,
    JSON.stringify(cr4Routes));
  check('CR4 recreate leg: the marker says leg=recreate and its counts match the submission',
    deepEq(captureLines(), ['N2H_DIAG_CAPTURE_ROUTE|requestId=' + REQUEST_ID +
      '|target=203.0.113.44|leg=recreate|result=added|reason=none|diag=true|is_evidence=false']) &&
    fieldOf(lastLine('VPN_EXCLUSION_GENERATION|'), 'routes') === String(cr4Routes.length) &&
    linesWith('VPN_ROUTE_ENTRY|').length === cr4Routes.length,
    JSON.stringify(captureLines()));

  // CR5: an EXCLUDED target is refused, never shadowed. The initial leg freezes
  // the management endpoint's /32 (203.0.113.7) as an isExcludedRoute entry; a
  // capture /32 for the same address would shadow that exclusion (longest
  // prefix wins), which is exactly the masking the plan forbids.
  const cr5 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({
    diag_capture_route: true,
    protect_probe: { target: '203.0.113.7:53', count: 1, mode: 'none' }
  }));
  resetLog();
  const cr5Submitted = await driveInitialLeg(cr5, NETCFG_0021);
  const cr5Frozen = (cr5['frozenEndpointExclusions'] ?? []) as Array<Record<string, unknown>>;
  const cr5Rows = routeRowsWith(cr5Submitted[0] ?? {}, '203.0.113.7');
  check('CR5 premise: this leg really froze 203.0.113.7 as an endpoint exclusion',
    cr5Frozen.filter((e: Record<string, unknown>): boolean =>
      e['destination'] === '203.0.113.7' && e['isExcludedRoute'] === true).length === 1,
    JSON.stringify(cr5Frozen));
  check('CR5 an excluded target is NOT injected: the submission stays the default composition, ' +
    'the single 203.0.113.7 entry stays the EXCLUDED one, and the line names target-excluded',
    cr5Submitted.length === 1 &&
    deepEq(cr5Submitted[0], platformOf(injectedOf(legacyInitialTunnelConfig(cr5, NETCFG_0021))['config'])) &&
    cr5Rows.length === 1 && cr5Rows[0]['isExcludedRoute'] === true &&
    deepEq(captureLines(), ['N2H_DIAG_CAPTURE_ROUTE|requestId=' + REQUEST_ID +
      '|target=203.0.113.7|leg=initial|result=skipped|reason=target-excluded|' +
      'diag=true|is_evidence=false']),
    JSON.stringify({ capture: captureLines(), rows: cr5Rows }));

  // CR6: the pure helper's strict acceptance (mirror of the core's
  // parse_protect_target) and its skip tokens.
  function capAt(target: unknown, routes: unknown[] = []): Record<string, unknown> {
    return (cfg['withCaptureRoute'] as
      (c: unknown, t: unknown) => Record<string, unknown>)(
      tunCfg('100.108.144.165', 16, routes), target);
  }
  const cr6InputRoutes: unknown[] = [];
  const cr6Added = capAt('203.0.113.9:53', cr6InputRoutes);
  check('CR6 a valid IPv4:port target appends the managed /32 and reports target/reason',
    cr6Added['added'] === true && cr6Added['target'] === '203.0.113.9' &&
    cr6Added['reason'] === 'none' &&
    deepEq(((cr6Added['config'] as Record<string, unknown>)['routes'] as unknown[]), [{
      destination: '203.0.113.9', prefixLength: 32, gateway: '100.108.144.165',
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: false
    }]) && cr6InputRoutes.length === 0,
    JSON.stringify(cr6Added));
  check('CR6 no / non-string target -> no-target (never a guessed destination)',
    capAt('')['reason'] === 'no-target' && capAt('')['target'] === '' &&
    capAt(undefined)['reason'] === 'no-target' && capAt(null)['reason'] === 'no-target' &&
    capAt(7)['reason'] === 'no-target',
    JSON.stringify([capAt('')['reason'], capAt(undefined)['reason']]));
  check('CR6 targets that the core would refuse are refused HERE too (invalid-target): no port, ' +
    'empty/zero/overflow/non-numeric port, leading-zero port, non-IPv4 host, IPv6, leading-zero ' +
    'octet, wrong octet count',
    capAt('203.0.113.9')['reason'] === 'invalid-target' &&
    capAt('203.0.113.9:')['reason'] === 'invalid-target' &&
    capAt(':53')['reason'] === 'invalid-target' &&
    capAt('203.0.113.9:0')['reason'] === 'invalid-target' &&
    capAt('203.0.113.9:65536')['reason'] === 'invalid-target' &&
    capAt('203.0.113.9:053')['reason'] === 'invalid-target' &&
    capAt('203.0.113.9:abc')['reason'] === 'invalid-target' &&
    capAt('010.0.0.1:53')['reason'] === 'invalid-target' &&
    capAt('256.1.1.1:53')['reason'] === 'invalid-target' &&
    capAt('1.2.3.4.5:53')['reason'] === 'invalid-target' &&
    capAt('mgmt.test:53')['reason'] === 'invalid-target' &&
    capAt('2001:db8::1:53')['reason'] === 'invalid-target' &&
    capAt('203.0.113.9:53')['added'] === true,
    JSON.stringify([capAt('203.0.113.9')['reason'], capAt('2001:db8::1:53')['reason'],
      capAt('010.0.0.1:53')['reason']]));
  const cr6Excl32 = [{ destination: '203.0.113.9', prefixLength: 32, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }];
  const cr6Excl24 = [{ destination: '203.0.113.0', prefixLength: 24, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }];
  const cr6ExclOther = [{ destination: '198.51.100.0', prefixLength: 24, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }];
  const cr6ExclV6 = [{ destination: '2001:db8::10', prefixLength: 128, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: true }];
  const cr6NonExcl = [{ destination: '203.0.113.9', prefixLength: 32, gateway: '10.99.0.1',
    hasGateway: true, isDefaultRoute: false, isExcludedRoute: false }];
  const cr6Skip32 = capAt('203.0.113.9:53', cr6Excl32);
  const cr6Skip24 = capAt('203.0.113.9:53', cr6Excl24);
  check('CR6 an excluded /32 AND an excluded covering /24 both refuse the target ' +
    '(target-excluded, config returned untouched), while another /24, an IPv6 /128 exclusion and ' +
    'a NON-excluded /32 do not',
    cr6Skip32['reason'] === 'target-excluded' && cr6Skip32['added'] === false &&
    cr6Skip32['target'] === '203.0.113.9' &&
    (cr6Skip32['config'] as Record<string, unknown>)['routes'] === cr6Excl32 &&
    cr6Skip24['reason'] === 'target-excluded' && cr6Skip24['added'] === false &&
    capAt('203.0.113.9:53', cr6ExclOther)['added'] === true &&
    capAt('203.0.113.9:53', cr6ExclV6)['added'] === true &&
    capAt('203.0.113.9:53', cr6NonExcl)['added'] === true &&
    capAt('203.0.113.9:53', cr6NonExcl)['reason'] === 'none',
    JSON.stringify({ a: cr6Skip32['reason'], b: cr6Skip24['reason'] }));

  // CR7: the switch ON with no usable target — a logged skip, never a guess.
  const cr7 = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({
    diag_capture_route: true,
    protect_probe: { count: 2, mode: 'none' }
  }));
  resetLog();
  const cr7Submitted = await driveInitialLeg(cr7, NETCFG_0021);
  check('CR7 switch ON but no string protect_probe.target: nothing is injected, the target field ' +
    'is empty and the line says reason=no-target',
    cr7Submitted.length === 1 &&
    deepEq(cr7Submitted[0], platformOf(injectedOf(legacyInitialTunnelConfig(cr7, NETCFG_0021))['config'])) &&
    cr7['diagCaptureTarget'] === '' &&
    deepEq(captureLines(), ['N2H_DIAG_CAPTURE_ROUTE|requestId=' + REQUEST_ID +
      '|target=|leg=initial|result=skipped|reason=no-target|diag=true|is_evidence=false']),
    JSON.stringify(captureLines()));

  // ---------------- PP (2026-09-26): the S7 protect-probe fd hand-off -------
  // The bypass experiment's protected arm (plan §4/§6 item 5): BEFORE the native
  // arm, `mode:"protect"` makes the shell run
  //   core.n2h_probe_socket_open() -> protectProbeSocketWithBox(platform
  //   connection.protect(fd), always) -> core.n2h_probe_socket_commit(fd,result)
  // and only then n2h_collector_arm(...). The fd is CORE-owned: the shell never
  // closes it, never duplicates it and never touches its flags. PP0 pins the
  // verbatim option passthrough; PP1 the control / absent cases (zero hand-off);
  // PP2 the settled order; PP3 that the route-exclusion protect DEVIATION does
  // not void the experiment; PP4-PP6 the four outcome mappings and the bounded
  // box (no residual timer); PP7 the open-failure path (still arms); PP8 the
  // refused commit; PP9 the fd-ownership pins.
  const PROTECT_PROBE_LINE = 'VPN_PROTECT_PROBE|';
  const PROBE_HANDOFF_ID = 'N2H-protect-0123456789abcdef01234567';

  function probeTrace(): string[] {
    return (G['__NB_PROBE_TRACE'] ?? []) as string[];
  }

  function clearProbeCounters(): void {
    G['__NB_PROBE_TRACE'] = [];
    G['__NB_PROBE_OPEN_CALLS'] = 0;
    G['__NB_PROBE_COMMIT_CALLS'] = 0;
    G['__NB_PROBE_COMMIT_ARGS'] = [];
    G['__NB_PROTECTED'] = 0;
    G['__NB_ARM_CALLS'] = 0;
    G['__NB_PROBE_OPEN_ANSWER'] = undefined;
    G['__NB_PROBE_COMMIT_ANSWER'] = undefined;
    G['__NB_PROBE_FD'] = undefined;
    G['__NB_PROTECT_MODE'] = 'resolved';
  }

  function protectProbeTicket(mode: string, target: string = '203.0.113.9:53',
    count: number = 3): Record<string, unknown> {
    return { target: target, count: count, mode: mode };
  }

  function probeTicketJson(probe: unknown, extra: Record<string, unknown> = {}): string {
    return JSON.stringify(Object.assign({}, extra, { protect_probe: probe }));
  }

  async function driveProbeArm(ability: Record<string, unknown>): Promise<string[]> {
    ability['connection'] = new FakeConnection();
    seedNetCtx({});
    resetLog();
    tickArm(ability, WG_READY);
    await drainProbe();
    return probeTrace();
  }

  function probeOpenFailureLine(error: string): string {
    return PROTECT_PROBE_LINE + 'requestId=' + REQUEST_ID +
      '|fd=-1|handoff_id=|outcome=open-failed|elapsedMs=0|commit=none|error=' + error + '|' +
      'diag=true|is_evidence=false';
  }

  // PP0: the verbatim passthrough of the two NEW arm keys, next to the
  // pre-existing ones (the shell's options document is built field by field,
  // so a missing forward would silently drop them — this is the assertion that
  // it does not).
  const pp0 = freshAbility(mod, FIXTURE_FILESDIR);
  pp0['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('none', '203.0.113.9:53', 5),
    { posctl_port: 41000, posctl_probes: 4 }));
  clearProbeCounters();
  await driveProbeArm(pp0);
  const pp0Opts = armOptsJson();
  check('PP0 posctl_probes and protect_probe ride the opts document VERBATIM (values and key ' +
    'order of the frozen set unchanged, no extra key invented)',
    pp0Opts['posctl_probes'] === 4 && pp0Opts['posctl_port'] === 41000 &&
    deepEq(pp0Opts['protect_probe'], protectProbeTicket('none', '203.0.113.9:53', 5)) &&
    deepEq(Object.keys(pp0Opts).sort(), ['exclusion_generation', 'posctl_port', 'posctl_probes',
      'probe_wait_ms', 'protect_probe', 'shell_exclusions']),
    JSON.stringify(pp0Opts));
  const pp0b = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(probeTicketJson(7, { posctl_probes: '4' }));
  clearProbeCounters();
  const pp0bTrace = await driveProbeArm(pp0b);
  const pp0bOpts = armOptsJson();
  check('PP0 wrong-typed values are forwarded AS THEY ARE (no coercion, no drop, no repair) and ' +
    'trigger no hand-off — the core\'s strict checks are the authority',
    pp0bOpts['posctl_probes'] === '4' && pp0bOpts['protect_probe'] === 7 &&
    deepEq(pp0bTrace, ['arm']) && Number(G['__NB_PROBE_OPEN_CALLS']) === 0,
    JSON.stringify({ opts: pp0bOpts, trace: pp0bTrace }));

  // PP1: the two zero-behaviour cases — mode "none" (the control arm) and an
  // absent protect_probe key.
  const pp1 = freshAbility(mod, FIXTURE_FILESDIR);
  pp1['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('none')));
  clearProbeCounters();
  const pp1Trace = await driveProbeArm(pp1);
  check('PP1 mode=none: ZERO open / protect / commit calls, no VPN_PROTECT_PROBE line, and the arm ' +
    'still runs exactly once with the object forwarded verbatim',
    deepEq(pp1Trace, ['arm']) && Number(G['__NB_PROBE_OPEN_CALLS']) === 0 &&
    Number(G['__NB_PROBE_COMMIT_CALLS']) === 0 && Number(G['__NB_PROTECTED']) === 0 &&
    Number(G['__NB_ARM_CALLS']) === 1 && pp1['n2hArmAttempted'] === true &&
    deepEq(armOptsJson()['protect_probe'], protectProbeTicket('none')) &&
    linesWith(PROTECT_PROBE_LINE).length === 0,
    JSON.stringify({ trace: pp1Trace, opts: armOptsJson() }));
  const pp1b = freshAbility(mod, FIXTURE_FILESDIR);
  setSwitchTicket(JSON.stringify({ posctl_port: 41000 }));
  clearProbeCounters();
  const pp1bTrace = await driveProbeArm(pp1b);
  check('PP1 no protect_probe key at all: both new keys stay ABSENT from the opts document and ' +
    'the hand-off never runs',
    deepEq(pp1bTrace, ['arm']) && !('protect_probe' in armOptsJson()) &&
    !('posctl_probes' in armOptsJson()) && Number(G['__NB_PROBE_OPEN_CALLS']) === 0 &&
    Number(G['__NB_PROBE_COMMIT_CALLS']) === 0,
    JSON.stringify(armOptsJson()));

  // PP2: the protected arm's order and outcome, on the very fd open returned.
  const pp2 = freshAbility(mod, FIXTURE_FILESDIR);
  pp2['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  G['__NB_PROBE_FD'] = 71;
  const pp2Trace = await driveProbeArm(pp2);
  check('PP2 the hand-off order is EXACTLY open -> protect(fd) -> commit(fd,protected) -> arm, on ' +
    'the fd n2h_probe_socket_open answered',
    deepEq(pp2Trace, ['open', 'protect:71', 'commit:71:protected', 'arm']),
    JSON.stringify(pp2Trace));
  check('PP2 the committed outcome is the platform settle (protected), the fd is echoed unchanged ' +
    'and the line is the fixed literal with the core\'s handoff_id',
    deepEq(G['__NB_PROBE_COMMIT_ARGS'], [[71, 'protected']]) &&
    Number(G['__NB_PROBE_OPEN_CALLS']) === 1 && Number(G['__NB_PROBE_COMMIT_CALLS']) === 1 &&
    Number(G['__NB_PROTECTED']) === 1 && Number(G['__NB_ARM_CALLS']) === 1 &&
    new RegExp('^VPN_PROTECT_PROBE\\|requestId=' + REQUEST_ID + '\\|fd=71\\|handoff_id=' +
      PROBE_HANDOFF_ID + '\\|outcome=settled\\|elapsedMs=\\d+\\|commit=ok\\|diag=true\\|' +
      'is_evidence=false$').test(lastLine(PROTECT_PROBE_LINE)) &&
    linesWith(PROTECT_PROBE_LINE).length === 1,
    JSON.stringify({ args: G['__NB_PROBE_COMMIT_ARGS'], line: lastLine(PROTECT_PROBE_LINE) }));

  // PP3: the DEVIATION must not void the experiment. With
  // protectDeviation=route-exclusion armed, protectSocketWithBox short-circuits
  // to true WITHOUT calling the platform; the dedicated wrapper must not.
  const pp3 = freshAbility(mod, FIXTURE_FILESDIR);
  pp3['n2hAcceptedExclusionGeneration'] = 0;
  pp3['protectDeviationRouteExclusion'] = true;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  G['__NB_PROBE_FD'] = 73;
  const pp3Trace = await driveProbeArm(pp3);
  check('PP3 with protectDeviation=route-exclusion ARMED the dedicated wrapper STILL calls the ' +
    'platform protect (counted, once) and settles — no deviation line, no silent no-op',
    deepEq(pp3Trace, ['open', 'protect:73', 'commit:73:protected', 'arm']) &&
    Number(G['__NB_PROTECTED']) === 1 && Number(G['__NB_PROBE_COMMIT_CALLS']) === 1 &&
    new RegExp('\\|fd=73\\|handoff_id=' + PROBE_HANDOFF_ID +
      '\\|outcome=settled\\|elapsedMs=\\d+\\|commit=ok\\|').test(lastLine(PROTECT_PROBE_LINE)) &&
    linesWith('VPN_PROTECT_DEVIATION|').length === 0,
    JSON.stringify({ trace: pp3Trace, line: lastLine(PROTECT_PROBE_LINE) }));

  // PP4/PP5: rejected -> unprotected, synchronous throw -> error.
  const pp4 = freshAbility(mod, FIXTURE_FILESDIR);
  pp4['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  G['__NB_PROTECT_MODE'] = 'rejected';
  G['__NB_PROBE_FD'] = 81;
  const pp4Trace = await driveProbeArm(pp4);
  check('PP4 a REJECTED platform protect maps to outcome=rejected / commit result=unprotected, ' +
    'and the arm still runs',
    deepEq(pp4Trace, ['open', 'protect:81', 'commit:81:unprotected', 'arm']) &&
    Number(G['__NB_ARM_CALLS']) === 1 &&
    new RegExp('^VPN_PROTECT_PROBE\\|requestId=' + REQUEST_ID + '\\|fd=81\\|handoff_id=' +
      PROBE_HANDOFF_ID + '\\|outcome=rejected\\|elapsedMs=\\d+\\|commit=ok\\|diag=true\\|' +
      'is_evidence=false$').test(lastLine(PROTECT_PROBE_LINE)),
    JSON.stringify({ trace: pp4Trace, line: lastLine(PROTECT_PROBE_LINE) }));
  const pp5 = freshAbility(mod, FIXTURE_FILESDIR);
  pp5['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  G['__NB_PROTECT_MODE'] = 'throw';
  G['__NB_PROBE_FD'] = 83;
  const pp5Trace = await driveProbeArm(pp5);
  check('PP5 a SYNCHRONOUS throw from the platform protect maps to outcome=throw / result=error ' +
    '(never a hang, never a false protected) and the arm still runs',
    deepEq(pp5Trace, ['open', 'protect:83', 'commit:83:error', 'arm']) &&
    Number(G['__NB_ARM_CALLS']) === 1 &&
    new RegExp('^VPN_PROTECT_PROBE\\|requestId=' + REQUEST_ID + '\\|fd=83\\|handoff_id=' +
      PROBE_HANDOFF_ID + '\\|outcome=throw\\|elapsedMs=\\d+\\|commit=ok\\|diag=true\\|' +
      'is_evidence=false$').test(lastLine(PROTECT_PROBE_LINE)),
    JSON.stringify({ trace: pp5Trace, line: lastLine(PROTECT_PROBE_LINE) }));

  // PP6: the bounded box, made deterministic. setTimeout/clearTimeout are
  // captured (the IB burst-spy pattern) so the 5 s expiry can be fired by hand
  // WITHOUT waiting, and so "the settled path cleared the handle" / "the timeout
  // path leaves no further timer or line behind" are assertions about the REAL
  // call sites. setImmediate is deliberately NOT captured: it is how the tests
  // drain the real promise chain without a timer.
  const ppTimers: Array<{ id: number; handler: () => void; ms: number; pending: boolean }> = [];
  const ppCleared: number[] = [];
  let ppNextTimer = 9000;
  const ppPrevSetTimeout = G['setTimeout'];
  const ppPrevClearTimeout = G['clearTimeout'];
  async function drainImmediate(): Promise<void> {
    await new Promise<void>((resolve: () => void): void => { setImmediate(resolve); });
  }
  G['setTimeout'] = (handler: () => void, ms?: number): number => {
    const id = ++ppNextTimer;
    ppTimers.push({ id: id, handler: handler, ms: ms ?? 0, pending: true });
    return id;
  };
  G['clearTimeout'] = (id: number): void => {
    ppCleared.push(id);
    const hit = ppTimers.find((t: { id: number }): boolean => t.id === id);
    if (hit !== undefined) {
      hit.pending = false;
    }
  };
  async function driveProbeArmTimed(ability: Record<string, unknown>): Promise<string[]> {
    ability['connection'] = new FakeConnection();
    seedNetCtx({});
    resetLog();
    tickArm(ability, WG_READY);
    await drainImmediate();
    return probeTrace();
  }
  const pp6 = freshAbility(mod, FIXTURE_FILESDIR);
  pp6['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  ppTimers.length = 0;
  ppCleared.length = 0;
  G['__NB_PROTECT_MODE'] = 'never';
  G['__NB_PROBE_FD'] = 91;
  const pp6TraceBefore = await driveProbeArmTimed(pp6);
  check('PP6 a never-settling protect parks the hand-off on EXACTLY ONE bounded timer (boxMs=5000) ' +
    'and the arm has not run yet',
    ppTimers.length === 1 && ppTimers[0].ms === 5000 && ppTimers[0].pending === true &&
    deepEq(pp6TraceBefore, ['open', 'protect:91']) &&
    linesWith(PROTECT_PROBE_LINE).length === 0,
    JSON.stringify({ timers: ppTimers.map((t): number => t.ms), trace: pp6TraceBefore }));
  ppTimers[0].handler(); // the box expires
  await drainImmediate();
  await drainImmediate();
  check('PP6 the expiry maps to outcome=timeout / commit result=error, the arm then runs, and ' +
    'NOTHING is left behind: no timer was re-armed, no second line appears and no callback of ' +
    'this hand-off can fire again',
    deepEq(probeTrace(), ['open', 'protect:91', 'commit:91:error', 'arm']) &&
    ppTimers.length === 1 &&
    Number(G['__NB_ARM_CALLS']) === 1 &&
    new RegExp('^VPN_PROTECT_PROBE\\|requestId=' + REQUEST_ID + '\\|fd=91\\|handoff_id=' +
      PROBE_HANDOFF_ID + '\\|outcome=timeout\\|elapsedMs=\\d+\\|commit=ok\\|diag=true\\|' +
      'is_evidence=false$').test(lastLine(PROTECT_PROBE_LINE)) &&
    linesWith(PROTECT_PROBE_LINE).length === 1,
    JSON.stringify({ trace: probeTrace(), timers: ppTimers.length,
      lines: linesWith(PROTECT_PROBE_LINE) }));
  // The settled half on the SAME spy: the box handle IS cleared, so the 5 s
  // expiry cannot later print a second, false outcome.
  const pp6b = freshAbility(mod, FIXTURE_FILESDIR);
  pp6b['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  ppTimers.length = 0;
  ppCleared.length = 0;
  G['__NB_PROBE_FD'] = 92;
  const pp6bTrace = await driveProbeArmTimed(pp6b);
  check('PP6b a settled protect CLEARS its box handle (the 2026-09-25 fix): the one timer armed ' +
    'for this hand-off is no longer pending, nothing is left to fire, and the outcome is settled',
    deepEq(pp6bTrace, ['open', 'protect:92', 'commit:92:protected', 'arm']) &&
    ppTimers.length === 1 && ppTimers[0].pending === false &&
    deepEq(ppCleared, [ppTimers[0].id]) &&
    new RegExp('\\|fd=92\\|handoff_id=' + PROBE_HANDOFF_ID +
      '\\|outcome=settled\\|elapsedMs=\\d+\\|commit=ok\\|').test(lastLine(PROTECT_PROBE_LINE)),
    JSON.stringify({ timers: ppTimers, cleared: ppCleared, trace: pp6bTrace }));
  G['setTimeout'] = ppPrevSetTimeout;
  G['clearTimeout'] = ppPrevClearTimeout;

  // PP7: an open failure is logged and the arm STILL runs (the core records the
  // never-committed hand-off itself); no protect and no commit may happen.
  const pp7 = freshAbility(mod, FIXTURE_FILESDIR);
  pp7['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  G['__NB_PROBE_OPEN_ANSWER'] =
    JSON.stringify({ ok: false, error: 'socket-open-failed', errno: 24 });
  const pp7Trace = await driveProbeArm(pp7);
  check('PP7 a refused open prints its own line (fd=-1, outcome=open-failed, commit=none, the ' +
    'core\'s error token), calls NO protect and NO commit, and the arm still runs exactly once',
    deepEq(pp7Trace, ['open', 'arm']) && Number(G['__NB_PROTECTED']) === 0 &&
    Number(G['__NB_PROBE_COMMIT_CALLS']) === 0 && Number(G['__NB_ARM_CALLS']) === 1 &&
    deepEq(linesWith(PROTECT_PROBE_LINE), [
      probeOpenFailureLine('socket-open-failed')
    ]) && armOptsJson()['protect_probe'] !== undefined,
    JSON.stringify({ trace: pp7Trace, lines: linesWith(PROTECT_PROBE_LINE) }));
  const pp7b = freshAbility(mod, FIXTURE_FILESDIR);
  pp7b['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  G['__NB_PROBE_OPEN_ANSWER'] = 'not-a-json-body';
  const pp7bTrace = await driveProbeArm(pp7b);
  check('PP7b an unparseable open answer is the SAME open-failed shape with ' +
    'error=open-rejected and codes/names only (never the raw body)',
    deepEq(pp7bTrace, ['open', 'arm']) && Number(G['__NB_PROTECTED']) === 0 &&
    linesWith(PROTECT_PROBE_LINE).length === 1 &&
    deepEq(linesWith(PROTECT_PROBE_LINE), [
      PROTECT_PROBE_LINE + 'requestId=' + REQUEST_ID + '|fd=-1|handoff_id=|outcome=open-failed|' +
      'elapsedMs=0|commit=none|error=open-rejected|summary=code=no-code,name=SyntaxError|' +
      'diag=true|is_evidence=false'
    ]) &&
    lastLine(PROTECT_PROBE_LINE).indexOf('not-a-json-body') < 0,
    lastLine(PROTECT_PROBE_LINE));

  // PP8: a REFUSED commit is recorded verbatim in `commit=` and the arm still
  // runs (the core, not the shell, decides what a refusal means).
  const pp8 = freshAbility(mod, FIXTURE_FILESDIR);
  pp8['n2hAcceptedExclusionGeneration'] = 0;
  setSwitchTicket(probeTicketJson(protectProbeTicket('protect')));
  clearProbeCounters();
  G['__NB_PROBE_FD'] = 95;
  G['__NB_PROBE_COMMIT_ANSWER'] = JSON.stringify({ ok: false, error: 'fd-invalid' });
  const pp8Trace = await driveProbeArm(pp8);
  check('PP8 a refused commit lands verbatim in the line (commit=fd-invalid) and does NOT stop ' +
    'the arm — the shell reports, the core judges',
    deepEq(pp8Trace, ['open', 'protect:95', 'commit:95:protected', 'arm']) &&
    Number(G['__NB_PROBE_COMMIT_CALLS']) === 1 && Number(G['__NB_ARM_CALLS']) === 1 &&
    new RegExp('^VPN_PROTECT_PROBE\\|requestId=' + REQUEST_ID + '\\|fd=95\\|handoff_id=' +
      PROBE_HANDOFF_ID + '\\|outcome=settled\\|elapsedMs=\\d+\\|commit=fd-invalid\\|diag=true\\|' +
      'is_evidence=false$').test(lastLine(PROTECT_PROBE_LINE)),
    JSON.stringify({ trace: pp8Trace, line: lastLine(PROTECT_PROBE_LINE) }));

  // PP9: the fd-ownership pins. The three methods of the hand-off may only ever
  // hand the fd to protect()/commit(): they hold no close, no duplicate and no
  // flag call, so the core remains the ONE owner (a shell-side close would make
  // the core's commit answer `fd-invalid` and silently turn the protected arm
  // into a no-op).
  const pp9HandoffMatch = /private async handOffProtectProbe\(\): Promise<void> \{[\s\S]*?\n  \}/
    .exec(extSource);
  const pp9WrapperMatch = /private async protectProbeSocketWithBox\([\s\S]*?\n  \}/.exec(extSource);
  const pp9CommitMatch =
    /private commitProtectProbe\(fd: number, result: string\): string \{[\s\S]*?\n  \}/
      .exec(extSource);
  const pp9Bodies = (pp9HandoffMatch === null ? '' : pp9HandoffMatch[0]) +
    (pp9WrapperMatch === null ? '' : pp9WrapperMatch[0]) +
    (pp9CommitMatch === null ? '' : pp9CommitMatch[0]);
  const pp9CoreCalls = Array.from(new Set(pp9Bodies.match(/core\.[A-Za-z0-9_]+/g) ?? [])).sort();
  check('PP9 the hand-off (open / protect wrapper / commit) exists, calls ONLY ' +
    'n2h_probe_socket_open + n2h_probe_socket_commit on the native side, and carries no ' +
    'close/dup/fcntl call on the fd',
    pp9HandoffMatch !== null && pp9WrapperMatch !== null && pp9CommitMatch !== null &&
    deepEq(pp9CoreCalls, ['core.n2h_probe_socket_commit', 'core.n2h_probe_socket_open']) &&
    pp9Bodies.indexOf('closeSync(') < 0 && pp9Bodies.indexOf('fcntl') < 0 &&
    !/close\(|dup\(/.test(pp9Bodies) &&
    extSource.indexOf("'VPN_PROTECT_PROBE|requestId=%{public}s|fd=-1|handoff_id=|outcome=open-failed|'") > 0,
    JSON.stringify({ calls: pp9CoreCalls, len: pp9Bodies.length }));

  // ---------------- N26-N30 (2026-09-30): the N2 C/D FORMAL-switch ----------
  // ---------------- production-recovery gate -------------------------------
  //
  // What is under test is ONE shell-side invariant, driven through the REAL
  // methods (pollN2CdPlan / physicalNetworkAvailable / pollN2CdStep /
  // recoverPhysicalNetwork / fulfillSocketRequest / performControlledRecreate /
  // readDiagSwitches / onDestroy) and measured by CALL COUNTS at the core seams
  // (`__NB_SUPPLY_*`, bumped by exclusion-generation-hooks.mjs):
  //
  //   after a C/D plan's FORMAL switch is reported, and until the core's own
  //   poll answers `step=recover` for THAT plan at THAT switch epoch, no
  //   recovery / new-connection supply entry may hand the core a production
  //   connection of the new network. Control markers are still sent (the shell
  //   opens them itself, never through a supply entry), the D preparation hop
  //   still recovers ordinarily, and an unarmed session has no gate at all.
  //
  // The CORE half (`n2_cd.rs`) is untouched; the hook's default C/D answers
  // mirror only the core preconditions the shell cannot see (D preparation
  // before the formal switch, DWELL_MIN_MS), so a fixture can never "switch" a
  // plan the real core would refuse.

  const D_PAYLOAD = '{"column":"D","kind":"M","rep":1,"id":"cd-D-M-1"}';
  const CD_ORDER = (): string => JSON.stringify(G['__NB_ORDER'] ?? []);
  const cdStepDoc = (step: string, phase: string, id: string, kind: string, column: string,
    marker: string, result: string, reason: string): string => JSON.stringify({
      ok: true, step: step, phase: phase, id: id, kind: kind, column: column,
      fd: -1, dup_fd: -1, remote: 'unknown', marker: marker, result: result,
      reason: reason, line: ''
    });
  const cdPlanDoc = (id: string): string => JSON.stringify({ ok: true, result: 'planned',
    reason: 'ok', id: id, pre_marker: id + '-pre', post_marker: id + '-post', line: '' });
  // The measured physical networks of this group. Wi-Fi 101 is the acceptance
  // link, 202 the D preparation (cellular) link, 303 the Wi-Fi the D column
  // returns to, 404 another Wi-Fi.
  const WiFiLink = (address: string): Record<string, unknown> => ({
    dnses: [{ address: '119.29.29.29', family: 1 }],
    linkAddresses: [{ address: { address: address, family: 1 } }]
  });
  const seedWifi = (netId: number, address: string, others: number[] = []): void => {
    const caps: Record<string, unknown> = {};
    const props: Record<string, unknown> = {};
    for (const id of others) {
      caps[id] = { networkCap: [15], bearerTypes: [1] };
      props[id] = WiFiLink('192.168.50.' + String(200 + (id % 50)));
    }
    caps[netId] = { networkCap: [15], bearerTypes: [1] };
    props[netId] = WiFiLink(address);
    seedNetCtx({ __NB_NET_DEFAULT_ID: netId,
      __NB_NET_ALL_IDS: others.concat([netId]), __NB_NET_CAPS_BY_ID: caps,
      __NB_NET_PROPS_BY_ID: props });
  };
  // The ability wired the way the create() flow leaves it (an accepted
  // generation with a live connection and the frozen management endpoint), so
  // recoverPhysicalNetwork can really run.
  const gateReadyAbility = (netId: number): Record<string, unknown> => {
    const ability = armedAbility();
    ability['networkRecoveryReady'] = true;
    ability['networkNetId'] = netId;
    ability['connection'] = new FakeConnection();
    ability['managementEndpoint'] = { host: 'mgmt.test', port: 33073, ip: '' };
    ability['signalUri'] = '';
    return ability;
  };
  const switchTo = (ability: Record<string, unknown>, eventNetId: number,
    netId: number): void => {
    (ability['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
      'netAvailable', eventNetId, netId);
  };

  // ---- N26: the D column, legal order, end to end ------------------------
  // prepare -> ready (+ DWELL_MIN_MS) -> a REAL in-flight recovery of the
  // preparation epoch (parked at the shell-side DNS await, with the
  // pending-retry path armed) -> the formal cellular -> Wi-Fi switch -> the old
  // await released. Before the core's `step=recover` the gate must let NOTHING
  // through; after it, exactly one recovery.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, D_PAYLOAD);
  seedWifi(101, '192.168.50.199');
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  G['__NB_CD_PLAN_ANSWER'] = cdPlanDoc('cd-D-M-1');
  const dGate = gateReadyAbility(101);
  resetLog();
  (dGate['pollN2CdPlan'] as () => void)();
  check('N26 the D plan is accepted as a PRE-SET plan (await-start-bearer) on the Wi-Fi link',
    Number(G['__NB_CD_PLAN_CALLS'] ?? 0) === 1 &&
    ((G['__NB_CD_PLAN_LAST'] ?? {}) as Record<string, unknown>)['column'] === 'D' &&
    ((G['__NB_CD_CORE'] ?? {}) as Record<string, unknown>)['phase'] === 'await-start-bearer' &&
    dGate['n2CdGateState'] === '' &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=plan|marker=cd-D-M-1-pre|result=planned|') > 0,
    JSON.stringify({ plan: G['__NB_CD_PLAN_LAST'], core: G['__NB_CD_CORE'],
      line: lastLine('N2_CD_SANDWICH_SHELL|') }));

  // The preparation hop (Wi-Fi -> cellular): recorded, NO control, and the
  // ordinary recovery runs and lands — this is what puts the plan in position.
  seedNetCtx({ __NB_NET_DEFAULT_ID: 202, __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: { 101: WiFiLink('192.168.50.199'),
      202: WiFiLink('10.77.0.5') } });
  resetLog();
  switchTo(dGate, 202, 202);
  await sleepMs(250);
  check('N26 the preparation hop recovers ORDINARILY (no gate, no control) and reports ready',
    dGate['n2CdPreparePending'] === false && controlCalls() === 0 &&
    ((G['__NB_CD_PREPARE_LAST'] ?? {}) as Record<string, unknown>)['verdict'] === '' &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf(
      '|phase=prepare|marker=none|result=ready|reason=preparation-recovered|') > 0 &&
    lastLine('VPN_NETWORK_RECOVERED|').indexOf('netId=202') > 0 &&
    supplyTotal() > 0,
    JSON.stringify({ prepare: G['__NB_CD_PREPARE_LAST'], supply: supplyLine(),
      line: lastLine('N2_CD_SANDWICH_SHELL|'), recovered: lastLine('VPN_NETWORK_RECOVERED|') }));
  const dReadyMs = Number(((G['__NB_CD_CORE'] ?? {}) as Record<string, unknown>)['readyMs'] ?? 0);
  check('N26 the preparation recovery really handed the core n2_cd_ready',
    Number(G['__NB_CD_READY_CALLS'] ?? 0) === 1 && dReadyMs > 0,
    JSON.stringify({ ready: G['__NB_CD_READY_LAST'], readyMs: dReadyMs }));

  // The DWELL_MIN_MS is satisfied, and a REAL in-flight recovery of the
  // preparation epoch is arranged: its shell-side DNS resolve is parked, and a
  // second entry (the same one the pending-retry / recreate-finally paths use)
  // arms the pending flag. Note WHY this needs the entry call: once a D plan is
  // Ready, every further MEASURED transition of that plan is either the formal
  // cellular -> Wi-Fi switch or a direction refusal, so the "older attempt still
  // in flight when the switch lands" state can only be reached through the real
  // recovery entry itself — exactly the state the pending-retry and
  // recreate-finally callers create in production.
  G['__NB_CD_CORE_NOW'] = dReadyMs + 3001;
  G['__NB_DNS_POLL'] = { ok: true, pending: true };
  resetSupply();
  (dGate['recoverPhysicalNetwork'] as () => Promise<void>)().catch(() => {});
  (dGate['recoverPhysicalNetwork'] as () => Promise<void>)().catch(() => {});
  await sleepMs(150);
  check('N26 a REAL recovery of the preparation epoch is genuinely in flight (parked at the ' +
    'DNS await) with the pending-retry path armed',
    dGate['networkRecoveryActive'] === true && dGate['networkRecoveryPending'] === true &&
    supplyOf('__NB_SUPPLY_DNS_RESOLVE') >= 1 && supplyOf('__NB_SUPPLY_WG_FEED') === 0,
    JSON.stringify({ active: dGate['networkRecoveryActive'],
      pending: dGate['networkRecoveryPending'], supply: supplyLine() }));

  // The FORMAL switch: the gate is armed, the old registration's identity is
  // captured and reported, the core's cleanup runs, and only THEN the pre.
  resetSupply();
  resetLog();
  G['__NB_ORDER'] = undefined;
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 31, dup_fd: 32,
    generation: 5, net_id: 202 });
  G['__NB_CD_POLL_ANSWER_QUEUE'] = [
    cdStepDoc('pre', '', 'cd-D-M-1', 'M', 'D', 'cd-D-M-1-pre', 'pre', 'send-pre-now'),
    cdStepDoc('wait', '', 'cd-D-M-1', 'M', 'D', '', 'wait', 'await-switch')
  ];
  seedWifi(303, '192.168.50.240', [101, 202]);
  const changedBefore = Number(G['__NB_PHYSICAL_CHANGED'] ?? 0);
  switchTo(dGate, 303, 303);
  check('N26 the formal switch arms the gate for THIS plan and THIS epoch, and reports the ' +
    'measured old-network identity',
    dGate['n2CdGateState'] === 'blocked' && dGate['n2CdGatePlan'] === 'cd-D-M-1' &&
    dGate['n2CdGateEpoch'] === dGate['networkEpoch'] &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['verdict'] === '' &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['netId'] === 303 &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['fd'] === 31 &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['dupFd'] === 32 &&
    ((G['__NB_CD_SWITCH_LAST'] ?? {}) as Record<string, unknown>)['generation'] === 5,
    JSON.stringify({ gate: [dGate['n2CdGateState'], dGate['n2CdGateEpoch'],
      dGate['networkEpoch']], sw: G['__NB_CD_SWITCH_LAST'] }));
  check('N26 the gate opens NO core supply at the switch instant',
    supplyTotal() === 0 && linesWith('VPN_NETWORK_RECOVERED|').length === 0 &&
    dGate['networkRecoveryActive'] === true,
    JSON.stringify({ supply: supplyLine(), recovered: linesWith('VPN_NETWORK_RECOVERED|') }));
  check('N26 the core\'s own old-connection cleanup still runs synchronously (it is not deferred)',
    Number(G['__NB_PHYSICAL_CHANGED'] ?? 0) === changedBefore + 1 && CD_ORDER().indexOf(
      '"physical-changed"') > CD_ORDER().indexOf('"identity"'), CD_ORDER());

  // The pre control is owed on the NEW network and must leave AFTER that
  // cleanup; the queued `wait` keeps the gate closed.
  await drainProbe();
  await drainProbe();
  check('N26 the pre control is still sent on the new network while the gate is CLOSED, and the ' +
    'core cleanup preceded it',
    controlCalls() === 1 &&
    JSON.stringify(G['__NB_CONTROL_IDS']) === JSON.stringify(['cd-D-M-1-pre']) &&
    ((G['__NB_CD_PRE_LAST'] ?? {}) as Record<string, unknown>)['result'] === 'hit' &&
    CD_ORDER() === JSON.stringify(['identity', 'physical-changed', 'control:cd-D-M-1-pre']),
    JSON.stringify({ control: G['__NB_CONTROL_LAST'], order: CD_ORDER() }));
  check('N26 the core\'s standing pre HIT alone releases NOTHING (no step=recover yet)',
    dGate['n2CdGateState'] === 'blocked' && supplyTotal() === 0 &&
    linesWith('VPN_NETWORK_RECOVERED|').length === 0,
    JSON.stringify({ gate: dGate['n2CdGateState'], supply: supplyLine() }));

  // The on-demand supply entry is refused for BOTH a DNS and a management
  // socket while the gate is closed — the legacy `isDns && networkRecoveryActive`
  // carve-out must not bypass it.
  resetSupply();
  await (dGate['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'gate-supply-m', kind: 'tcp', purpose: 'management' });
  await (dGate['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'gate-supply-d', kind: 'udp', purpose: 'dns' });
  const gateSupplyLines = linesWith('VPN_SOCKET_SUPPLY|');
  check('N26b while the gate is closed the on-demand supply entry refuses BOTH the management ' +
    'and the protected-DNS socket (reason=production-gate)',
    gateSupplyLines.length === 2 &&
    gateSupplyLines.every((l: string): boolean => l.indexOf('|reason=production-gate|') > 0) &&
    supplyOf('__NB_SUPPLY_MGMT_OPEN') === 0 && supplyOf('__NB_SUPPLY_ICE_OPEN') === 0 &&
    supplyOf('__NB_SUPPLY_DNS') === 0 && supplyTotal() === 0,
    JSON.stringify({ lines: gateSupplyLines, supply: supplyLine() }));

  // Release the OLD await. The in-flight attempt is now stale; the gate must
  // stop it from chasing the new epoch, and the finally's pending retry must be
  // refused too. This is the exact defect the independent review found.
  resetSupply();
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  await sleepMs(300);
  check('N26 after the old await is released the in-flight recovery ABANDONS the gated epoch: ' +
    'zero new-network supply, no RECOVERED, gate still CLOSED',
    dGate['networkRecoveryActive'] === false && dGate['networkRecoveryPending'] === false &&
    dGate['n2CdGateState'] === 'blocked' && supplyTotal() === 0 &&
    linesWith('VPN_NETWORK_RECOVERED|').length === 0,
    JSON.stringify({ supply: supplyLine(), gate: dGate['n2CdGateState'],
      recovered: linesWith('VPN_NETWORK_RECOVERED|') }));

  // The ONLY release: the core's own poll naming this plan at this epoch.
  resetSupply();
  resetLog();
  G['__NB_CD_POLL_ANSWER'] = cdStepDoc('recover', 'released', 'cd-D-M-1', 'M', 'D', '',
    'recover', 'pre-hit-observed');
  (dGate['pollN2CdStep'] as () => void)();
  await sleepMs(300);
  check('N26 the core\'s step=recover releases the gate for its own epoch and drives EXACTLY ' +
    'one production recovery',
    dGate['n2CdGateState'] === 'released' &&
    supplyOf('__NB_SUPPLY_DNS_RESOLVE') === 1 && supplyOf('__NB_SUPPLY_WG_OPEN') === 1 &&
    supplyOf('__NB_SUPPLY_WG_FEED') === 1 && supplyOf('__NB_SUPPLY_MGMT_FEED') === 1 &&
    supplyOf('__NB_SUPPLY_ICE_FEED') >= 1 && supplyOf('__NB_SUPPLY_DNS') >= 1 &&
    linesWith('VPN_NETWORK_RECOVERED|').length === 1 &&
    lastLine('VPN_NETWORK_RECOVERED|').indexOf('netId=303') > 0,
    JSON.stringify({ gate: dGate['n2CdGateState'], supply: supplyLine(),
      recovered: linesWith('VPN_NETWORK_RECOVERED|') }));
  check('N26 the released recovery ran AFTER the pre control (the sandwich order holds)',
    CD_ORDER() === JSON.stringify(['identity', 'physical-changed', 'control:cd-D-M-1-pre',
      'recover-feed']), CD_ORDER());
  stopAbilityTimers(dGate);

  // ---- N27: a formal switch BEFORE the preparation recovery landed ---------
  // The core closes the plan `unsat|switch-before-preparation-complete`. The
  // shell must NOT drop the latch and let the ordinary recovery pollute the
  // failed epoch: the gate goes to HELD and stays there until the evidence
  // session is explicitly terminated.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, D_PAYLOAD);
  seedWifi(101, '192.168.50.199');
  G['__NB_CD_PLAN_ANSWER'] = cdPlanDoc('cd-D-M-1');
  G['__NB_DNS_POLL'] = { ok: true, pending: true };
  const dHeld = gateReadyAbility(101);
  (dHeld['pollN2CdPlan'] as () => void)();
  seedNetCtx({ __NB_NET_DEFAULT_ID: 202, __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: { 101: WiFiLink('192.168.50.199'), 202: WiFiLink('10.77.0.5') } });
  resetLog();
  switchTo(dHeld, 202, 202);
  await sleepMs(150);
  check('N27 the preparation hop is recorded and its recovery is genuinely in flight',
    Number(G['__NB_CD_PREPARE_CALLS'] ?? 0) === 1 && dHeld['n2CdPreparePending'] === true &&
    dHeld['networkRecoveryActive'] === true,
    JSON.stringify({ prepare: G['__NB_CD_PREPARE_LAST'],
      active: dHeld['networkRecoveryActive'] }));

  resetSupply();
  resetLog();
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 41, dup_fd: 42,
    generation: 6, net_id: 202 });
  seedWifi(303, '192.168.50.240', [101, 202]);
  switchTo(dHeld, 303, 303);
  check('N27 the formal switch before the preparation recovery is refused by the core by name, ' +
    'drops the shell latch, and HOLDS the gate',
    Number(G['__NB_CD_SWITCH_CALLS'] ?? 0) === 1 &&
    dHeld['n2CdActive'] === '' && dHeld['n2CdGateState'] === 'held' &&
    lastLine('N2_CD_SANDWICH_SHELL|').indexOf('|phase=switch|marker=none|result=unsat|' +
      'reason=switch-before-preparation-complete|') > 0 &&
    supplyTotal() === 0,
    JSON.stringify({ gate: dHeld['n2CdGateState'], active: dHeld['n2CdActive'],
      line: lastLine('N2_CD_SANDWICH_SHELL|'), supply: supplyLine() }));

  resetSupply();
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  await sleepMs(300);
  check('N27 releasing the old await cannot make the failed epoch recover (held gate, no supply)',
    dHeld['networkRecoveryActive'] === false && dHeld['n2CdGateState'] === 'held' &&
    supplyTotal() === 0 && linesWith('VPN_NETWORK_RECOVERED|').length === 0,
    JSON.stringify({ supply: supplyLine(), gate: dHeld['n2CdGateState'] }));

  // A LATER switch and direct recovery entries must not release it either.
  resetSupply();
  seedWifi(404, '192.168.50.77', [101, 202, 303]);
  switchTo(dHeld, 404, 404);
  await sleepMs(150);
  (dHeld['recoverPhysicalNetwork'] as () => Promise<void>)().catch(() => {});
  await sleepMs(150);
  check('N27 the terminal UNSAT does NOT self-release: a later switch and a direct recovery ' +
    'entry leave the gate HELD and supply nothing',
    dHeld['n2CdGateState'] === 'held' && supplyTotal() === 0 &&
    linesWith('VPN_NETWORK_RECOVERED|').length === 0 &&
    (dHeld['n2CdProductionBlocked'] as () => boolean)() === true,
    JSON.stringify({ gate: dHeld['n2CdGateState'], supply: supplyLine(),
      recovered: linesWith('VPN_NETWORK_RECOVERED|') }));

  // The EXPLICIT termination: a fresh start flow without the arm ticket
  // (readDiagSwitches) resets the gate; the product recovers again.
  setArmFile(false, undefined);
  (dHeld['readDiagSwitches'] as () => void)();
  check('N28 the evidence session\'s explicit termination (readDiagSwitches) clears the gate',
    dHeld['n2CdGateState'] === '' && dHeld['n2Evidence'] === undefined &&
    (dHeld['n2CdProductionBlocked'] as () => boolean)() === false,
    JSON.stringify({ gate: dHeld['n2CdGateState'], evidence: dHeld['n2Evidence'] }));
  resetSupply();
  resetLog();
  dHeld['networkDirty'] = true;
  (dHeld['recoverPhysicalNetwork'] as () => Promise<void>)().catch(() => {});
  await sleepMs(300);
  check('N28 after the reset an ordinary recovery supplies normally again (the product is not ' +
    'blocked by a terminated session)',
    supplyOf('__NB_SUPPLY_WG_FEED') === 1 &&
    linesWith('VPN_NETWORK_RECOVERED|').length === 1,
    JSON.stringify({ supply: supplyLine(), recovered: linesWith('VPN_NETWORK_RECOVERED|') }));

  // Teardown is NEVER gated: the stop latches and the destroy leg runs.
  (dHeld['onDestroy'] as () => void)();
  await drainProbe();
  resetSupply();
  await (dHeld['fulfillSocketRequest'] as (r: unknown) => Promise<void>)(
    { id: 'stop-supply', kind: 'tcp', purpose: 'management' });
  check('N28 teardown is not gated: onDestroy latches the stop, the destroy leg runs, and the ' +
    'supply refusal is the teardown one (destroy), not a gate',
    dHeld['destroyRequested'] === true &&
    lastLine('VPN_DESTROY_SKIPPED|').length > 0 &&
    lastLine('VPN_SOCKET_SUPPLY|').indexOf('|reason=destroy|') > 0,
    JSON.stringify({ requested: dHeld['destroyRequested'],
      destroy: lastLine('VPN_DESTROY_SKIPPED|'), supply: lastLine('VPN_SOCKET_SUPPLY|') }));
  stopAbilityTimers(dHeld);

  // ---- N29: the recreate-finally retry is a supply entry too --------------
  // A real C plan switch arms the gate; networkDirty stays true (the gated
  // re-feed never ran). The controlled recreate completes — its TUN feed is not
  // a network supply — but its finally retry must NOT recover while the gate is
  // closed, and must recover once the core releases the epoch.
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  seedWifi(101, '192.168.50.199');
  G['__NB_CD_PLAN_ANSWER'] = cdPlanDoc('cd-C-M-1');
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  const recGate = gateReadyAbility(101);
  (recGate['pollN2CdPlan'] as () => void)();
  seedNetCtx({ __NB_NET_DEFAULT_ID: 202, __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: { 101: WiFiLink('192.168.50.199'), 202: WiFiLink('10.77.0.5') } });
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 51, dup_fd: 52,
    generation: 7, net_id: 101 });
  G['__NB_CD_POLL_ANSWER_QUEUE'] = [
    cdStepDoc('pre', '', 'cd-C-M-1', 'M', 'C', 'cd-C-M-1-pre', 'pre', 'send-pre-now'),
    cdStepDoc('wait', '', 'cd-C-M-1', 'M', 'C', '', 'wait', 'await-switch')
  ];
  switchTo(recGate, 202, 202);
  await drainProbe();
  await drainProbe();
  check('N29 the C plan\'s formal switch arms the gate and blocks the re-feed',
    recGate['n2CdGateState'] === 'blocked' && recGate['networkDirty'] === true &&
    supplyTotal() === 0,
    JSON.stringify({ gate: recGate['n2CdGateState'], dirty: recGate['networkDirty'],
      supply: supplyLine() }));

  resetSupply();
  resetLog();
  await driveRecreate(recGate);
  check('N29 the recreate leg itself completes while the gate is closed (its TUN feed is not a ' +
    'new-network supply), and its finally retry is GATED OFF',
    ((G['__NB_ACK'] ?? {}) as Record<string, unknown>)['isRecreate'] === true &&
    recGate['recreateInFlight'] === false && recGate['networkRecoveryActive'] === false &&
    supplyTotal() === 0 && linesWith('VPN_NETWORK_RECOVERED|').length === 0,
    JSON.stringify({ ack: G['__NB_ACK'], supply: supplyLine(),
      recovered: linesWith('VPN_NETWORK_RECOVERED|') }));

  // Positive control: the same path DOES recover once the core released it.
  G['__NB_CD_POLL_ANSWER'] = cdStepDoc('recover', 'released', 'cd-C-M-1', 'M', 'C', '',
    'recover', 'pre-hit-observed');
  resetSupply();
  resetLog();
  (recGate['pollN2CdStep'] as () => void)();
  await sleepMs(300);
  check('N29 control: once the core answers step=recover the very same recreate-finally path ' +
    'recovers (so the zero above is the GATE, not an unrelated failure)',
    recGate['n2CdGateState'] === 'released' && supplyOf('__NB_SUPPLY_WG_FEED') === 1 &&
    linesWith('VPN_NETWORK_RECOVERED|').length === 1,
    JSON.stringify({ gate: recGate['n2CdGateState'], supply: supplyLine() }));
  stopAbilityTimers(recGate);

  // ---- N30: the release is bound to plan AND epoch ------------------------
  resetFs();
  resetRequestState();
  seedFile(CD_PATH, CD_PAYLOAD);
  seedWifi(101, '192.168.50.199');
  G['__NB_CD_PLAN_ANSWER'] = cdPlanDoc('cd-C-M-1');
  G['__NB_DNS_POLL'] = { ok: true, pending: false, ip: '203.0.113.7' };
  const bindGate = gateReadyAbility(101);
  (bindGate['pollN2CdPlan'] as () => void)();
  seedNetCtx({ __NB_NET_DEFAULT_ID: 202, __NB_NET_ALL_IDS: [101, 202],
    __NB_NET_CAPS_BY_ID: { 101: { networkCap: [15], bearerTypes: [1] },
      202: { networkCap: [15], bearerTypes: [0] } },
    __NB_NET_PROPS_BY_ID: { 101: WiFiLink('192.168.50.199'), 202: WiFiLink('10.77.0.5') } });
  G['__NB_CD_IDENTITY_ANSWER'] = JSON.stringify({ ok: true, fd: 61, dup_fd: 62,
    generation: 8, net_id: 101 });
  G['__NB_CD_POLL_ANSWER_QUEUE'] = [
    cdStepDoc('pre', '', 'cd-C-M-1', 'M', 'C', 'cd-C-M-1-pre', 'pre', 'send-pre-now'),
    cdStepDoc('wait', '', 'cd-C-M-1', 'M', 'C', '', 'wait', 'await-switch')
  ];
  switchTo(bindGate, 202, 202);
  await drainProbe();
  await drainProbe();
  const bindEpoch = Number(bindGate['networkEpoch']);
  check('N30 the binding fixture arms the gate at one epoch',
    bindGate['n2CdGateState'] === 'blocked' && bindGate['n2CdGateEpoch'] === bindEpoch,
    JSON.stringify({ gate: bindGate['n2CdGateState'], epoch: bindEpoch }));

  // (a) a standing permission naming ANOTHER plan releases nothing.
  resetSupply();
  G['__NB_CD_POLL_ANSWER'] = cdStepDoc('recover', 'released', 'cd-C-M-9', 'M', 'C', '',
    'recover', 'pre-hit-observed');
  (bindGate['pollN2CdStep'] as () => void)();
  await drainProbe();
  check('N30 a step=recover naming ANOTHER plan releases nothing (no recovery, zero supply)',
    bindGate['n2CdGateState'] === 'blocked' && bindGate['networkRecoveryActive'] === false &&
    supplyTotal() === 0,
    JSON.stringify({ gate: bindGate['n2CdGateState'], supply: supplyLine() }));

  // (b) the epoch moves (a measured network loss bumps it) and the SAME plan's
  // standing permission must still not release the NEW epoch.
  (bindGate['physicalNetworkAvailable'] as (e: string, en: number, n: number) => void)(
    'netLost', 0, 0);
  const movedEpoch = Number(bindGate['networkEpoch']);
  resetSupply();
  G['__NB_CD_POLL_ANSWER'] = cdStepDoc('recover', 'released', 'cd-C-M-1', 'M', 'C', '',
    'recover', 'pre-hit-observed');
  (bindGate['pollN2CdStep'] as () => void)();
  await drainProbe();
  check('N30 a standing step=recover that arrives AFTER another measured switch releases ' +
    'nothing (the release is bound to the switch epoch)',
    movedEpoch === bindEpoch + 1 && bindGate['n2CdGateState'] === 'blocked' &&
    bindGate['networkRecoveryActive'] === false && supplyTotal() === 0,
    JSON.stringify({ epochs: [bindEpoch, movedEpoch], gate: bindGate['n2CdGateState'],
      supply: supplyLine() }));
  stopAbilityTimers(bindGate);

  // ---- N30b: unarmed / startup leaves NO gate -----------------------------
  resetFs();
  resetRequestState();
  seedWifi(505, '192.168.50.88');
  const startupAbility = freshAbility(mod, FIXTURE_FILESDIR);
  startupAbility['networkRecoveryReady'] = true;
  startupAbility['networkNetId'] = 505;
  startupAbility['connection'] = new FakeConnection();
  startupAbility['managementEndpoint'] = { host: 'mgmt.test', port: 33073, ip: '' };
  startupAbility['signalUri'] = '';
  startupAbility['networkDirty'] = true;
  resetSupply();
  resetLog();
  (startupAbility['recoverPhysicalNetwork'] as () => Promise<void>)().catch(() => {});
  await sleepMs(300);
  check('N30b an unarmed session carries no gate and its (startup) recovery supplies normally',
    startupAbility['n2CdGateState'] === '' &&
    (startupAbility['n2CdProductionBlocked'] as () => boolean)() === false &&
    supplyOf('__NB_SUPPLY_WG_FEED') === 1 &&
    linesWith('VPN_NETWORK_RECOVERED|').length === 1,
    JSON.stringify({ gate: startupAbility['n2CdGateState'], supply: supplyLine() }));
  stopAbilityTimers(startupAbility);
  resetSupply();

  rmSync(dir, { recursive: true, force: true });
} catch (error) {
  failedCount++;
  console.log('FAIL harness — ' + (error as Error).message + '\n' +
    ((error as Error).stack ?? ''));
}

console.log('');
console.log(String(passed) + ' passed, ' + String(failedCount) + ' failed');
process.exit(failedCount === 0 ? 0 : 1);
