// Loader-side resolve hook for run-relay-exclusion.mts ONLY (host-side
// fixture). Same principle as relay-flag-hooks.mjs / route-check-hooks.mjs:
// the real modules import @ohos.* / libnetbird_core.so which cannot resolve
// under plain node; this hook short-circuits ONLY those specifiers.
//
// The stubs are FUNCTIONAL, not empty, because the fixture imports the REAL
// NetBirdVpnExtensionAbility.ets and drives its private
// deriveEndpointExclusions() end-to-end:
//   - @ohos.hilog      -> recording no-op formatter; every formatted line
//                       lands in globalThis.__NB_LOG (log-marker assertions
//                       run against the REAL hilog call sites)
//   - @ohos.net.connection -> getAddressesByName reads the per-test fake DNS
//                       table globalThis.__NB_DNS (deterministic, offline;
//                       a missing host rejects exactly like the platform);
//                       the v3.1 dns caliber's four synchronous network APIs
//                       (getDefaultNetSync / getNetCapabilitiesSync /
//                       getConnectionPropertiesSync / getAllNetsSync) read the
//                       per-test tables below, so the REAL branch order can be
//                       driven end-to-end:
//                         globalThis.__NB_NET_DEFAULT_ID  default handle netId (0 = none)
//                         globalThis.__NB_NET_ALL         array of netIds for getAllNetsSync
//                         globalThis.__NB_NET_CAPS        { <netId>: {networkCap:[], bearerTypes:[]} }
//                         globalThis.__NB_NET_DNSES       { <netId>: [{address,family,port}] }
//                         globalThis.__NB_NET_THROW       array of method names to throw for
//                                                         (BusinessError-shaped, code 2100002)
//   - libnetbird_core.so -> connector_status() returns the per-test fake
//                       status JSON globalThis.__NB_FAKE_STATUS
//   - @ohos.app.ability.VpnExtensionAbility -> a bare base class so the real
//                       extension class can be `new`-ed on the host
// Everything else (@ohos.file.fs / .util / .resourceManager / .net.vpnExtension
// / @ohos.app.ability.Want) is dead code for the tested path -> empty stubs.
//
// Sibling modules are NOT stubbed: the test stages the five REAL files
// byte-identically in one temp dir. Type-only named imports (erased
// interfaces that would fail the ESM link) are bridged by the test-written
// <Name>.shim.ts files: `export * from './<Name>.ts'` (the real byte-identical
// fixture) plus undefined consts for the erased interface names. The hook
// redirects ONLY the extension './<Name>' specifiers to those shims; the
// fixtures themselves are never rewritten.

const EMPTY = 'data:text/javascript,export%20default%20%7B%7D%3B';

const HILOG = 'data:text/javascript,' + encodeURIComponent(
  'function fmt(f, a) { if (!a || a.length === 0) { return String(f); }\n' +
  '  let i = 0;\n' +
  '  return String(f).replace(/%\\{public\\}[sd]/g, () => (i < a.length ?' +
  ' String(a[i++]) : "")); }\n' +
  'const rec = (lvl) => (domain, tag, format, ...args) => {\n' +
  '  const g = globalThis;\n' +
  '  g.__NB_LOG = g.__NB_LOG || [];\n' +
  '  g.__NB_LOG.push("[" + lvl + "] " + fmt(format, args));\n' +
  '};\n' +
  'export default { debug: rec("D"), info: rec("I"), warn: rec("W"),' +
  ' error: rec("E") };');

const CONNECTION = 'data:text/javascript,' + encodeURIComponent(
  'function netG() { return globalThis; }\n' +
  'function netThrowing(name) {\n' +
  '  const t = netG().__NB_NET_THROW;\n' +
  '  return Array.isArray(t) && t.indexOf(name) >= 0;\n' +
  '}\n' +
  'function netBoom(name) {\n' +
  '  const e = new Error("net-api:" + name);\n' +
  '  e.code = 2100002;\n' +
  '  throw e;\n' +
  '}\n' +
  'function netCaps(id) {\n' +
  '  const t = netG().__NB_NET_CAPS || {};\n' +
  '  const c = t[id];\n' +
  '  return c === undefined || c === null ? { networkCap: [], bearerTypes: [] } : c;\n' +
  '}\n' +
  'function netDnses(id) {\n' +
  '  const t = netG().__NB_NET_DNSES || {};\n' +
  '  const d = t[id];\n' +
  '  return d === undefined || d === null ? [] : d;\n' +
  '}\n' +
  'export default {\n' +
  '  getAddressesByName: (host) => {\n' +
  '    const t = globalThis.__NB_DNS || {};\n' +
  '    const v = t[host];\n' +
  '    return v !== undefined && v !== null\n' +
  '      ? Promise.resolve([{ address: v }])\n' +
  '      : Promise.reject(new Error("dns-failed"));\n' +
  '  },\n' +
  '  getDefaultNetSync: () => {\n' +
  '    if (netThrowing("getDefaultNetSync")) { netBoom("getDefaultNetSync"); }\n' +
  '    return { netId: netG().__NB_NET_DEFAULT_ID || 0 };\n' +
  '  },\n' +
  '  getNetCapabilitiesSync: (handle) => {\n' +
  '    if (netThrowing("getNetCapabilitiesSync")) { netBoom("getNetCapabilitiesSync"); }\n' +
  '    return netCaps(handle.netId);\n' +
  '  },\n' +
  '  getConnectionPropertiesSync: (handle) => {\n' +
  '    if (netThrowing("getConnectionPropertiesSync")) { netBoom("getConnectionPropertiesSync"); }\n' +
  // N2 C/D first batch: `linkAddresses` is the raw platform fact the pure
  // `classifyN2Bearer` records as `local=`; an unseeded netId answers none.
  '    const props = (netG().__NB_NET_PROPS_BY_ID || {})[handle.netId] || {};\n' +
  '    return { interfaceName: "wlan0", dnses: netDnses(handle.netId),\n' +
  '      linkAddresses: props.linkAddresses || [] };\n' +
  '  },\n' +
  '  getAllNetsSync: () => {\n' +
  '    if (netThrowing("getAllNetsSync")) { netBoom("getAllNetsSync"); }\n' +
  '    const all = netG().__NB_NET_ALL || [];\n' +
  '    const out = [];\n' +
  '    for (let i = 0; i < all.length; i++) { out.push({ netId: all[i] }); }\n' +
  '    return out;\n' +
  '  }\n' +
  '};');

const CORE = 'data:text/javascript,' + encodeURIComponent(
  'export default {\n' +
  '  connector_status: () => {\n' +
  '    const s = globalThis.__NB_FAKE_STATUS;\n' +
  '    return typeof s === "string" ? s : JSON.stringify(s === undefined ? {} : s);\n' +
  '  },\n' +
  // Explicit logout (2026-10-01): same loud-refusal rule — these suites never
  // write a `netbird-logout` ticket, so the poll never reaches the seam.
  '  connector_logout: () => JSON.stringify({ ok: false, error: "no-connector",\n' +
  '    reason: "not-running" }),\n' +
  // N2 T5a (2026-09-28): the shell's armed-only stimulus entry. These suites
  // never arm the evidence switch, so the poll never reaches it; the export
  // exists so an accidental call fails loudly with the honest not-armed answer
  // instead of a TypeError.
  '  n2_stimulus: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed", target_fd: -1 }),\n' +
  // N2 P4/P5 (2026-09-28): the same loud-refusal rule for the A-column trigger
  // and the sandwich seams — these suites never arm either.
  '  n2_a_event: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    kind: "none", fd: -1, dup_fd: -1 }),\n' +
  '  n2_sandwich_pre: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    kind: "none", marker: "none" }),\n' +
  '  n2_sandwich_post: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    kind: "none", marker: "none" }),\n' +
  '  n2_sandwich_poll: () => JSON.stringify({ ok: true, posts: [] }),\n' +
  // N2 first batch C/D columns (2026-09-29): the switching-sandwich seams and
  // the evidence ring. These suites never arm the evidence switch, so the plan
  // poll never reaches the core; the exports exist so an accidental call fails
  // loudly with the honest refusal instead of a TypeError. The ring calls are
  // silent no-ops (an unarmed session configures no ring).
  '  n2_cd_plan: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    id: "none", pre_marker: "none", post_marker: "none" }),\n' +
  '  n2_cd_pre: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    id: "none" }),\n' +
  '  n2_cd_prepare: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    id: "none" }),\n' +
  '  n2_cd_ready: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    id: "none" }),\n' +
  '  n2_cd_identity: () => JSON.stringify({ ok: false, fd: -1, dup_fd: -1, generation: -1,\n' +
  '    net_id: 0 }),\n' +
  '  n2_cd_switch_identified: () => JSON.stringify({ ok: false, result: "rejected",\n' +
  '    reason: "not-armed", id: "none" }),\n' +
  '  n2_cd_post: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    id: "none" }),\n' +
  '  n2_cd_poll: () => JSON.stringify({ ok: true, step: "idle", phase: "", id: "", kind: "",\n' +
  '    column: "", fd: -1, dup_fd: -1, remote: "unknown", marker: "", result: "", line: "" }),\n' +
  '  n2_ring_configure: () => JSON.stringify({ ok: true }),\n' +
  '  n2_ring_append: () => JSON.stringify({ ok: true, enabled: false }),\n' +
  '  n2_ring_disable: () => JSON.stringify({ ok: true }),\n' +
  '  n2_ring_status: () => JSON.stringify({ ok: true, enabled: false, line: "" }),\n' +
  '  n2_ring_manifest: () => JSON.stringify({ ok: true, line: "" }),\n' +
  '  n2_ring_dump: () => JSON.stringify({ ok: false, result: "rejected",\n' +
  '    reason: "not-configured", from_seq: 0, to_seq: 0, count: 0, emitted: 0,\n' +
  '    lines: [], summary: "" }),\n' +
  '  n2_ring_ack: () => JSON.stringify({ ok: false, result: "rejected", reason: "not-armed",\n' +
  '    session: "", up_to_seq: 0, acked_seq: 0, previous_acked_seq: 0, advanced: false })\n' +
  '};');

const VPN_ABILITY = 'data:text/javascript,' + encodeURIComponent(
  'export default class VpnExtensionAbility {}');

// @ohos.net.statistics: the extension now imports the iface-bytes probe module
// (2026-09-18 availability probe), whose specifier must resolve for the REAL
// extension fixture to link. These suites never arm the collector, so the
// probe never RUNS here — the stub exists so an accidental run fails loudly
// (rejected promise, documented internal-error code) instead of faking a
// value. Same shape as exclusion-generation-hooks.mjs without the recorder.
const STATISTICS = 'data:text/javascript,' + encodeURIComponent(
  'function boom(nic) {\n' +
  '  const e = new Error("statistics unset: " + nic); e.code = 2100003;\n' +
  '  return Promise.reject(e);\n' +
  '}\n' +
  'export default {\n' +
  '  getIfaceRxBytes: (nic) => boom(nic),\n' +
  '  getIfaceTxBytes: (nic) => boom(nic)\n' +
  '};');

// 2026-10-08 diagnostics wiring: the REAL extension imports the shell-side
// diagnostics writer (start/event/stop) and the system-event watcher
// (start/stop). Neither takes any part in the endpoint/relay exclusion
// derivation this fixture drives, and both need platform IO (@ohos.hidebug /
// @ohos.hiviewdfx.hiAppEvent) that this host fixture cannot run — so each
// specifier resolves to a no-op data module carrying exactly the named exports
// the extension imports by value (its PHONE_LOG result/summary interfaces are
// type-only and therefore erased by node's type stripping; the modules' own
// behaviour is host-checked by run-diagnostics.mts / run-system-events.mts and
// run-phone-log.mts, never here).
const DIAG_NOOP = 'data:text/javascript,' + encodeURIComponent(
  'export const start = () => undefined;\n' +
  'export const stop = () => undefined;\n' +
  'export const event = () => false;\n' +
  'export const appendSystemEvent = () => false;\n' +
  'export const isRunning = () => false;\n');

const ACCOUNT_MODE = 'data:text/javascript,' + encodeURIComponent(
  'export const ACCOUNT_MODE_ACCOUNT = "account";\n' +
  'export const ACCOUNT_MODE_FILE_NAME = "netbird-auth-mode";\n' +
  'export const ACCOUNT_MODE_SETUP_KEY = "setup-key";\n' +
  'export const parseAuthMode = (text) => !text || !text.trim() || text.trim() === "setup-key"' +
  ' ? "setup-key" : text.trim() === "account" ? "account" : "invalid";\n' +
  'export const decideAccountCredentialMode = () => "account-login-required";');
const ACCOUNT_STORE = 'data:text/javascript,export%20async%20function%20loadAccountCredentials()%7Breturn%20null%3B%7D';

const STUBS = {
  '@ohos.app.ability.VpnExtensionAbility': VPN_ABILITY,
  '@ohos.app.ability.Want': EMPTY,
  // 2026-10-08: the extension's onMemoryLevel signature names
  // AbilityConstant.MemoryLevel; the import must resolve for the staged fixture
  // to link (the enum is never read at runtime on any path this suite drives).
  '@ohos.app.ability.AbilityConstant': EMPTY,
  '@ohos.file.fs': EMPTY,
  '@ohos.hilog': HILOG,
  // 2026-10-09 (0.0.17) onDestroy-entry stop marker: the REAL extension now
  // imports `process` for the pid it stamps into the marker (the same
  // `process.pid` NetBirdDiagnostics.ets writes), so the specifier must resolve
  // for the byte-identical extension fixture to link. No path this suite drives
  // reaches the marker write (onCreate is never called here, and the write is
  // skipped unless the process armed the diagnostics); the value is a FIXED
  // host pid so an accidental call still leaves an honest line.
  '@ohos.process': 'data:text/javascript,' + encodeURIComponent(
    'export default { pid: 4242 };\n'),
  '@ohos.net.connection': CONNECTION,
  '@ohos.net.statistics': STATISTICS,
  '@ohos.net.vpnExtension': EMPTY,
  '@ohos.resourceManager': EMPTY,
  '@ohos.util': EMPTY,
  'libnetbird_core.so': CORE,
  './NetBirdDiagnostics': DIAG_NOOP,
  './NetBirdSystemEvents': DIAG_NOOP,
  '../login/NetBirdAccountMode': ACCOUNT_MODE,
  '../login/NetBirdAccountStore': ACCOUNT_STORE,
  // 2026-10-07 per-NetID route selection (decision B): the REAL extension now
  // imports the route file adapter by value, so the specifier must resolve for
  // the extension fixture to link. This suite never calls it (it drives
  // deriveEndpointExclusions only) and stages no route document, so the answer
  // is the truthful "no stored selection" (null = the pre-existing default
  // all-on start) and a catalog write that IS reached fails LOUDLY instead of
  // faking a published document. The real adapter + its rules are driven
  // end-to-end by run-route-vpe.mts (and run-route-preferences.mts), never
  // mirrored here.
  '../routes/NetBirdRouteStore': 'data:text/javascript,' + encodeURIComponent(
    'export const loadRouteSelection = async () => null;\n' +
    'export const writeRouteCatalog = async () => {\n' +
    '  throw new Error("route-catalog-write-not-staged-in-relay-exclusion-fixture");\n' +
    '};'),
  // 2026-10-09 live route apply: the REAL extension now imports the live-route
  // IPC file adapter (and the state/mode token vocabulary of its pure document
  // module) by value, so both specifiers must resolve for this fixture to link.
  // This suite never drives the apply consumer (its 100 ms tick is never
  // reached here), so the request read is the truthful "no request" and a
  // result write fails LOUDLY rather than pretending an answer was published.
  // The real adapter and its documents are driven end-to-end by
  // run-route-apply-vpe.mts, never mirrored here.
  '../routes/NetBirdRouteRequestStore': 'data:text/javascript,' + encodeURIComponent(
    'export const readRouteApplyRequest = async () => null;\n' +
    'export const writeRouteApplyResult = async () => {\n' +
    '  throw new Error("route-apply-result-not-staged-in-relay-exclusion-fixture");\n' +
    '};'),
  '../routes/NetBirdRouteRequest': 'data:text/javascript,' + encodeURIComponent(
    'export const ROUTE_APPLY_STATE_APPLYING = "applying";\n' +
    'export const ROUTE_APPLY_STATE_APPLIED = "applied";\n' +
    'export const ROUTE_APPLY_STATE_FAILED = "failed";\n' +
    'export const ROUTE_APPLY_MODE_NONE = "";\n' +
    'export const ROUTE_APPLY_MODE_NO_OS_CHANGE = "no-os-change";\n' +
    'export const ROUTE_APPLY_MODE_RECREATED = "recreated";\n' +
    'export const ROUTE_APPLY_MODE_STORED_NO_MAP = "stored-no-map";')
};

// Sibling modules of the extension: the ESM link would fail on the erased
// type-only named imports, so the hook redirects them to the shim files the
// test writes next to the byte-identical fixtures. NetBirdInterfaceStatsProbe
// has no erased types (the extension imports only its one exported function),
// so its entry is plain extension resolution for the extensionless specifier.
const SHIMS = {
  './NetBirdConnector': 'NetBirdConnector.shim.ts',
  './NetBirdEndpointExclusion': 'NetBirdEndpointExclusion.shim.ts',
  './NetBirdRouteValidation': 'NetBirdRouteValidation.shim.ts',
  './NetBirdVpnConfig': 'NetBirdVpnConfig.shim.ts',
  './NetBirdInterfaceStatsProbe': 'NetBirdInterfaceStatsProbe.ts',
  './NetBirdPhysicalNetworkPolicy': 'NetBirdPhysicalNetworkPolicy.ts',
  // N5c demand-sized ICE pool (2026-10-02): zero imports, no erased types —
  // plain extension resolution like NetBirdInterfaceStatsProbe above.
  './NetBirdIceSocketPool': 'NetBirdIceSocketPool.ts',
  // UI phase 1 (2026-10-05): the status-document builder the extension
  // imports by value; its two erased interfaces are re-declared by the shim.
  './NetBirdUiModel': 'NetBirdUiModel.shim.ts',
  // roam-1005-0014: the recreate lifecycle policy pure module the REAL
  // extension imports; zero imports and no erased types — plain extension
  // resolution like NetBirdIceSocketPool above.
  './NetBirdRecreatePolicy': 'NetBirdRecreatePolicy.ts',
  // 2026-10-09 (0.0.17): the REAL extension now names the pure marker core by
  // value for its synchronous onDestroy-entry write, so the fixture stages that
  // file byte-identically next to the extension (zero imports, no erased types,
  // no shim needed — same discipline as NetBirdRecreatePolicy above).
  './NetBirdDiagnosticsCore': 'NetBirdDiagnosticsCore.ts',
  // N2 T5a (2026-09-28): the stimulus pure module the REAL extension imports;
  // its erased request/reply interfaces are re-declared by the shim.
  './NetBirdN2Stimulus': 'NetBirdN2Stimulus.shim.ts',
  // N2 T3 §3 (2026-09-28): the positive-control request pure module, staged and
  // shimmed exactly like the stimulus module above.
  './NetBirdN2ControlRequest': 'NetBirdN2ControlRequest.shim.ts',
  // N2 P4/P5 (2026-09-28): the A-column trigger and sandwich pure modules.
  './NetBirdN2AEvent': 'NetBirdN2AEvent.shim.ts',
  './NetBirdN2Sandwich': 'NetBirdN2Sandwich.shim.ts',
  // N2 first batch C/D columns (2026-09-29): the switching-sandwich pure module
  // and the ring pure module, staged and shimmed like the modules above.
  './NetBirdN2CdPlan': 'NetBirdN2CdPlan.shim.ts',
  './NetBirdN2Ring': 'NetBirdN2Ring.shim.ts'
};

export function resolve(specifier, context, next) {
  if (specifier === './NetBirdN2Control') {
    // N2 T3 §3 (2026-09-28): functional and recording like the other seams —
    // the REAL extension's armed-only `pollN2Control` imports this sender.
    return { url: 'data:text/javascript,' + encodeURIComponent(
      'export const N2ControlProtocol = undefined;\n' +
      'export const sendN2Control = (armed, target, protocol, id) => {\n' +
      '  const g = globalThis;\n' +
      '  g.__NB_CONTROL_CALLS = (g.__NB_CONTROL_CALLS || 0) + 1;\n' +
      '  g.__NB_CONTROL_LAST = { armed: armed, target: target, protocol: protocol, id: id };\n' +
      '  if (armed === undefined) { throw new Error("n2-not-armed"); }\n' +
      '  if ((g.__NB_CONTROL_MODE || "sent") === "throw") {\n' +
      '    throw new Error(g.__NB_CONTROL_ERROR || "n2-control-send-failed");\n' +
      '  }\n' +
      '  return undefined;\n' +
      '};\n' +
      'export const sendAndWaitN2Control = async (armed, target, protocol, id) => {\n' +
      '  sendN2Control(armed, target, protocol, id);\n' +
      '  return undefined;\n' +
      '};'), shortCircuit: true };
  }
  if (specifier === './NetBirdN2Evidence') {
    return { url: 'data:text/javascript,' + encodeURIComponent(
      'export const N2EvidenceOptions = undefined; export const N2EvidenceDecision = undefined; ' +
      'export const parseN2Evidence = () => ({ options: undefined }); ' +
      'export const withN2EvidenceRoutes = (config) => config; ' +
      'export const withoutN2ExcludedRoutes = (config) => config;'), shortCircuit: true };
  }
  if (Object.prototype.hasOwnProperty.call(STUBS, specifier)) {
    return { url: STUBS[specifier], shortCircuit: true };
  }
  if (Object.prototype.hasOwnProperty.call(SHIMS, specifier) &&
    context.parentURL !== undefined) {
    const dir = new URL('.', context.parentURL);
    return { url: new URL(SHIMS[specifier], dir).href, shortCircuit: true };
  }
  return next(specifier, context);
}