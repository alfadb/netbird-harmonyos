// 路由选择 INSTANT-apply self-check (2026-10-07, host-side; no device, no
// network, no build, no compiler). Same principle as run-route-preferences.mts
// and run-route-request.mts: the REAL production files are copied byte-for-byte
// into an OS-temp fixture (.ts) at run time and executed under plain node — the
// code under test is the shipped code, never a hand-held mirror:
//   routes/NetBirdRouteApplyPolicy.ets      (the pure status/wording policy)
//   routes/NetBirdRoutePreferences.ets      (pure selection state + documents)
//   routes/NetBirdRouteRequest.ets          (the live-route IPC documents + the
//                                            ONE "really applied" predicate)
//   routes/NetBirdRouteStore.ets            (the scoped preference, atomic IO)
//   routes/NetBirdRouteRequestStore.ets     (the request/result pair, atomic IO)
// The @ohos.* platform modules are functional stubs over the real node fs
// (route-apply-hooks.mjs), and the page methods driven below are sliced out of
// the byte-identical Index.ets — so the sequence is the SHIPPED page code.
//
// Coverage (task mapping):
//   P  the pure view policy: idle / saving / save-failed / notify-failed /
//      pending / applied / next-connect / failed, the tone of each, and the one
//      honesty rule (a matching id + the applied predicate + a live document).
//   R  the request id: fresh, unique (the counter breaks same-millisecond ties),
//      opaque.
//   H  rows: one HA NetID is ONE row carrying its subnet(s) and 节点数.
//   O  ONE switch action: the preference document is written BEFORE the doorbell,
//      the request is a pure notification (no selection copy, no session), and
//      the id is the page's own newest.
//   S  fast toggles: at most one write in flight at any moment, the LAST desired
//      selection wins on disk, a superseded wish is never announced, and no
//      `.tmp` sibling survives.
//   F  a FAILED preference write stores nothing, rings no doorbell and leaves the
//      running connection untouched; the page recovers on the next action.
//   G  the in-flight selection read cannot clobber a switch the user already
//      moved, while with no user edit it still seeds the page.
//   Q  the result document: an older id is ignored, applying is not 已生效, a
//      matching applied answer is, stored-no-map is NOT, a matching failure is
//      reported with its own reason, and the page's verdicts come from the
//      selection — never from the catalog's own `enabled` column.
//   L  offline: the preference AND the notification are still written and the
//      wording is 下次连接时生效; offline never claims 已生效; an unwritable
//      doorbell is reported as such.
//   T  the row switch over the REAL onChange body: a re-emitted value changes
//      nothing, a real change flips exactly once, a programmatic duplicate
//      callback is idempotent.
//   A  全部启用 / 全部停用 write through at once and keep their GLOBAL meaning.
//   N  no stop / start / re-login path anywhere in the route page, the retired
//      save/stop/ACK machinery is gone, no second timer, and the page never
//      touches the IPC paths directly.
//   W  wording/back: the instant-and-may-briefly-interrupt note, no 保存/放弃更改
//      button, back returns to 设置 without discarding anything, palette colours
//      only, no search widening.
//   E  返回后重新进入 (re-entry) while a read or a write is still in flight: the
//      wish is never clobbered by an older stored document, the edit protection
//      is not dropped by opening the page again, and a doorbell that could not
//      be written is not forgotten either. The same three sequences are replayed
//      against a throwaway pre-fix fixture (E9..E11), so the checks are shown to
//      fail without the fix.
//
// Run: node client/entry/selfcheck/run-route-apply.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, mkdirSync, rmSync, readdirSync, existsSync, statSync } from 'node:fs';
import { open, rename, unlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, basename } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./route-apply-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const ROUTES = join(ETS, 'routes');
const PAGE = readFileSync(join(ETS, 'pages', 'Index.ets'), 'utf8');
const POLICY_SOURCE = readFileSync(join(ROUTES, 'NetBirdRouteApplyPolicy.ets'), 'utf8');

const MGMT = 'https://management.example.com';
const SELECTION_FILE = 'netbird-route-selection.json';
const REQUEST_FILE = 'netbird-route-apply-request.json';
const RESULT_FILE = 'netbird-route-apply-result.json';

let passed = 0;
let failed = 0;
let fixtureDir = '';
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) { console.log(`     ${detail}`); }
}

function deepEq(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function sliceFrom(src: string, begin: string, end: string): string {
  const at = src.indexOf(begin);
  if (at < 0) { return ''; }
  const stop = src.indexOf(end, at);
  return stop < 0 ? src.slice(at) : src.slice(at, stop);
}

// Brace-matched slice of one real page method, from its signature through its
// closing brace (the `private ` modifier is dropped so the text can sit in an
// object literal). Same extraction principle as run-navigation-back.mts, so the
// sequence driven below is the SHIPPED page code, never a hand-held mirror.
function pageMethod(signature: string): string {
  const at = PAGE.indexOf(signature);
  if (at < 0) { throw new Error(`page method missing: ${signature}`); }
  const open = PAGE.indexOf('{', at);
  let depth = 0;
  for (let i = open; i < PAGE.length; i++) {
    const ch = PAGE[i];
    if (ch === '{') { depth++; }
    else if (ch === '}') {
      depth--;
      if (depth === 0) { return PAGE.slice(at, i + 1).replace(/^private\s+/, ''); }
    }
  }
  throw new Error(`unbalanced page method: ${signature}`);
}

// The statements between the braces of one inline callback body.
function blockBody(src: string, anchor: string): string {
  const at = src.indexOf(anchor);
  if (at < 0) { throw new Error(`callback anchor missing: ${anchor}`); }
  const open = src.indexOf('{', at);
  let depth = 0;
  for (let i = open; i < src.length; i++) {
    const ch = src[i];
    if (ch === '{') { depth++; }
    else if (ch === '}') {
      depth--;
      if (depth === 0) { return src.slice(open + 1, i); }
    }
  }
  throw new Error(`unbalanced callback body: ${anchor}`);
}

const tick = (): Promise<void> => new Promise<void>((resolve): void => { setTimeout(resolve, 1); });

try {
  fixtureDir = mkdtempSync(join(tmpdir(), 'nb-route-apply-'));
  const filesDir = join(fixtureDir, 'files');
  mkdirSync(filesDir);

  // byte-identical copies of the five shipped modules
  const staged: string[] = ['NetBirdRouteApplyPolicy', 'NetBirdRoutePreferences',
    'NetBirdRouteRequest', 'NetBirdRouteStore', 'NetBirdRouteRequestStore'];
  for (const name of staged) {
    writeFileSync(join(fixtureDir, `${name}.ts`), readFileSync(join(ROUTES, `${name}.ets`)));
  }

  // ---- the page stub: the REAL methods + the REAL routeRow onChange body ----
  const stubMethods: string[] = [
    pageMethod('private routeApplyAction(selection: RouteSelection): void'),
    pageMethod('private routeToggle(netId: string): void'),
    pageMethod('private routeEnableAll(): void'),
    pageMethod('private routeDisableAll(): void'),
    pageMethod('private routePersist(selection: RouteSelection): void'),
    pageMethod('private routePumpWrites(): void'),
    pageMethod('private routeSendApplyRequest(): Promise<void>'),
    pageMethod('private routeApplyView(): RouteApplyView'),
    pageMethod('private routeStatusText(): string'),
    pageMethod('private routeStatusTone(): string'),
    pageMethod('private refreshRouteApplyResult(): void'),
    pageMethod('private refreshRouteCatalog(): void'),
    pageMethod('private loadRouteSelection(): void'),
    pageMethod('private openRouteSelection(): void'),
    pageMethod('private routeRows(): RouteNetworkEntry[]'),
    pageMethod('private routeEnabledCount(): number'),
    pageMethod('private routeSummaryText(): string')
  ];
  const onChangeBody: string = blockBody(PAGE, '.onChange((isOn: boolean): void => {');
  const depsOf: string[] = [
    'hilog', 'DOMAIN', 'TAG', 'getContext', 'saveRouteSelection', 'writeRouteApplyRequest',
    'readRouteApplyResult', 'loadRouteSelection', 'routeSelectionToggle',
    'routeSelectionKnownNetIds', 'routeSelectionIsEnabled', 'routeSelectionDefault',
    'routeSelectionClone', 'routeSelectionSelectAll', 'routeSelectionDeselectAll',
    'RoutePreferencesError', 'REASON_UNKNOWN_ROUTE', 'routeSelectionRows',
    'routeApplyRequestId', 'routeApplyStatus', 'routeApplyStatusText',
    'routeApplyStatusTone', 'routeApplyResultIsApplied', 'ROUTE_TONE_ERROR',
    'readRouteCatalog', 'CATALOG_STATUS_UNAVAILABLE'
  ];
  // One builder for both fixtures: the SHIPPED page stub and the throwaway
  // negative control below differ only in the two method texts handed in.
  const stubSource = (methods: string[]): string => [
    'export function buildPage(deps) {',
    ...depsOf.map((name: string): string => `  const ${name} = deps.${name};`),
    '  return {',
    ...methods.map((m: string): string => `    ${m},`),
    '    routeToggleChange(isOn, entry) {',
    ...onChangeBody.split('\n').map((line: string): string => `  ${line}`),
    '    },',
    '    configuredUrl: "",',
    '    routeDraft: routeSelectionDefault(),',
    '    routeSelectionOpen: false,',
    '    routeLoadError: false, routeLoadErrorText: "", routeCatalog: null,',
    '    routeCatalogStatus: "", routeCatalogUrl: "", routeCatalogUpdatedAtMs: 0,',
    '    routeRequestId: "", routeResultId: "", routeResultState: "",',
    '    routeResultApplied: false, routeResultError: "",',
    '    routeSaving: false, routeSaveFailed: false, routeNotifyFailed: false,',
    '    routeActionError: "", routeUserEdited: false,',
    '    routeCatalogReading: false, routeResultReading: false, routeSelectionLoading: false,',
    '    routeWriteBusy: false, routeWritePending: null, routeRequestSeq: 0,',
    '    published: null',
    '  };',
    '}',
    ''
  ].join('\n');
  writeFileSync(join(fixtureDir, 'IndexPageStub.ts'), stubSource(stubMethods));

  // ---- the throwaway negative control (never written back to the repo) -----
  // The same shipped methods with EXACTLY the 2026-10-07 re-entry fix removed:
  // the edit protection is dropped on re-entry, the doorbell-failure state is
  // cleared on re-entry, and the selection read is no longer held back by an
  // in-flight write. This fixture exists only to prove the new checks FAIL
  // against the pre-fix page; if any replacement misses, it throws instead of
  // silently passing.
  const dropFix = (src: string, from: string, to: string): string => {
    if (src.indexOf(from) < 0) {
      throw new Error(`negative control: fix text not found in one page method: ${from}`);
    }
    return src.replace(from, to);
  };
  const oldMethods: string[] = stubMethods.map((m: string): string => {
    if (m.indexOf('openRouteSelection(): void {') === 0) {
      return dropFix(dropFix(m,
        '    if (!this.routeWriteBusy && !this.routeSelectionLoading) {\n' +
        '      this.routeUserEdited = false;\n' +
        '    }', '    this.routeUserEdited = false;'),
        '    this.loadRouteSelection();',
        '    this.routeNotifyFailed = false;\n    this.loadRouteSelection();');
    }
    if (m.indexOf('loadRouteSelection(): void {') === 0) {
      return dropFix(m, 'this.routeSelectionLoading || this.routeWriteBusy',
        'this.routeSelectionLoading');
    }
    return m;
  });
  writeFileSync(join(fixtureDir, 'IndexPageStubOld.ts'), stubSource(oldMethods));

  // The functional @ohos.file.fs stub forwards to this real node fs; the rename
  // recorder is the ORDER evidence (the preference must be renamed into place
  // before the request temp file even exists). statSync is the live-route request
  // adapter's cheap "unchanged document" probe (NetBirdRouteRequestStore), over
  // the same real fs with inode and nanosecond stamps.
  const renames: string[] = [];
  const state: any = {
    nodeFs: {
      open: open,
      rename: async (from: string, to: string): Promise<void> => {
        renames.push(basename(from));
        return rename(from, to);
      },
      unlink: unlink,
      statSync: statSync
    },
    readError: null,
    writeError: null
  };
  (globalThis as any).__NB_ROUTE_APPLY_TEST = state;

  const policy = await import(pathToFileURL(join(fixtureDir, 'NetBirdRouteApplyPolicy.ts')).href) as any;
  const prefs = await import(pathToFileURL(join(fixtureDir, 'NetBirdRoutePreferences.ts')).href) as any;
  const request = await import(pathToFileURL(join(fixtureDir, 'NetBirdRouteRequest.ts')).href) as any;
  const store = await import(pathToFileURL(join(fixtureDir, 'NetBirdRouteStore.ts')).href) as any;
  const requestStore = await import(
    pathToFileURL(join(fixtureDir, 'NetBirdRouteRequestStore.ts')).href) as any;
  const pageFixture = await import(pathToFileURL(join(fixtureDir, 'IndexPageStub.ts')).href) as any;
  const oldFixture = await import(pathToFileURL(join(fixtureDir, 'IndexPageStubOld.ts')).href) as any;

  check('every staged module fixture is byte-identical to the shipped file',
    staged.every((name: string): boolean =>
      readFileSync(join(ROUTES, `${name}.ets`)).equals(readFileSync(join(fixtureDir, `${name}.ts`)))));
  check('the page methods and the row callback under test were sliced out of the shipped Index.ets',
    stubMethods.every((m: string): boolean => m.length > 0) && onChangeBody.length > 0 &&
    onChangeBody.includes('this.routeToggle(entry.net_id)'));

  // ---- the recorded page doubles -------------------------------------------
  let writeCalls = 0;
  let requestCalls = 0;
  let inFlight = 0;
  let maxInFlight = 0;
  let notifyFail = false;
  let loadDeferred = false;
  let loadRelease: ((value: any) => void) | null = null;
  let loadCalls = 0;
  let saveDeferred = false;
  let saveRelease: (() => void) | null = null;

  const saveRouteSelection = async (dir: string, url: string, selection: any): Promise<void> => {
    writeCalls++;
    inFlight++;
    maxInFlight = Math.max(maxInFlight, inFlight);
    try {
      if (saveDeferred) {
        // The preference write is HELD: the re-entry sequences below need the
        // write to be genuinely in flight while the page is opened again.
        await new Promise<void>((resolve: () => void): void => { saveRelease = resolve; });
      }
      await store.saveRouteSelection(dir, url, selection);
    } finally {
      inFlight--;
    }
  };
  const writeRouteApplyRequest = async (dir: string, req: any): Promise<void> => {
    requestCalls++;
    if (notifyFail) { throw new Error('doorbell unavailable'); }
    await requestStore.writeRouteApplyRequest(dir, req);
  };
  const loadRouteSelection = (dir: string, url: string): Promise<any> => {
    loadCalls++;
    if (!loadDeferred) { return store.loadRouteSelection(dir, url); }
    return new Promise<any>((resolve): void => { loadRelease = resolve; });
  };

  const pageDeps: any = {
    hilog: { info: (): void => {}, warn: (): void => {}, error: (): void => {} },
    DOMAIN: 'selfcheck',
    TAG: 'selfcheck',
    getContext: (_page: any): any => ({ filesDir: filesDir }),
    saveRouteSelection: saveRouteSelection,
    writeRouteApplyRequest: writeRouteApplyRequest,
    readRouteApplyResult: (dir: string): Promise<any> => requestStore.readRouteApplyResult(dir),
    loadRouteSelection: loadRouteSelection,
    routeSelectionToggle: prefs.routeSelectionToggle,
    routeSelectionKnownNetIds: prefs.routeSelectionKnownNetIds,
    routeSelectionIsEnabled: prefs.routeSelectionIsEnabled,
    routeSelectionDefault: prefs.routeSelectionDefault,
    routeSelectionClone: prefs.routeSelectionClone,
    routeSelectionSelectAll: prefs.routeSelectionSelectAll,
    routeSelectionDeselectAll: prefs.routeSelectionDeselectAll,
    RoutePreferencesError: prefs.RoutePreferencesError,
    REASON_UNKNOWN_ROUTE: prefs.REASON_UNKNOWN_ROUTE,
    routeSelectionRows: prefs.routeSelectionRows,
    routeApplyRequestId: policy.routeApplyRequestId,
    routeApplyStatus: policy.routeApplyStatus,
    routeApplyStatusText: policy.routeApplyStatusText,
    routeApplyStatusTone: policy.routeApplyStatusTone,
    routeApplyResultIsApplied: request.routeApplyResultIsApplied,
    ROUTE_TONE_ERROR: policy.ROUTE_TONE_ERROR,
    readRouteCatalog: (dir: string, url: string): Promise<any> => store.readRouteCatalog(dir, url),
    CATALOG_STATUS_UNAVAILABLE: prefs.CATALOG_STATUS_UNAVAILABLE
  };
  const stub: any = (pageFixture.buildPage as any)(pageDeps);
  const oldStub: any = (oldFixture.buildPage as any)(pageDeps);
  // The re-entry sequences below also drive the throwaway pre-fix fixture, so the
  // waiter takes the page it should watch (the shipped stub by default).
  const settle = async (p: any = stub): Promise<void> => {
    for (let i = 0; i < 1000; i++) {
      if (!p.routeWriteBusy && p.routeWritePending === null &&
        !p.routeResultReading && !p.routeSelectionLoading && !p.routeCatalogReading) {
        return;
      }
      await tick();
    }
    throw new Error('the page never settled');
  };

  const catalog: any = {
    available: true,
    serial: 7,
    selection: { selected_routes: [], deselected_routes: [], deselect_all: false },
    networks: [
      { net_id: 'HostInternal', networks: ['192.168.50.0/24'], candidate_count: 2,
        enabled: true, is_exit: false },
      { net_id: 'exit-node', networks: ['0.0.0.0/0'], candidate_count: 1,
        enabled: true, is_exit: true }
    ]
  };
  const prefText = (): string | null => {
    const path = join(filesDir, SELECTION_FILE);
    return existsSync(path) ? readFileSync(path, 'utf8') : null;
  };
  const storedSelection = (): any => {
    const text = prefText();
    return text === null ? null : prefs.parseRouteSelectionDocument(text).selection;
  };
  const requestText = (): string | null => {
    const path = join(filesDir, REQUEST_FILE);
    return existsSync(path) ? readFileSync(path, 'utf8') : null;
  };
  const noTempResidue = (): boolean =>
    readdirSync(filesDir).every((name: string): boolean => !name.endsWith('.tmp'));

  // ---- P: the pure view policy --------------------------------------------
  const status = policy.routeApplyStatus as (view: any) => string;
  const statusText = policy.routeApplyStatusText as (s: string, e: string) => string;
  const statusTone = policy.routeApplyStatusTone as (s: string) => string;
  const view = (over: any = {}): any => Object.assign({
    latest_request_id: 'r1', result_id: '', result_state: '', result_applied: false,
    has_live_document: true, saving: false, save_failed: false, notify_failed: false
  }, over);

  check('P1 no request of ours says nothing at all (idle renders an empty line)',
    status(view({ latest_request_id: '' })) === 'idle' && statusText('idle', '') === '');
  check('P2 an in-flight write is 正在保存, and a failed write outranks every other state',
    status(view({ saving: true })) === 'saving' &&
    statusText('saving', '') === '正在保存…' &&
    status(view({ saving: true, save_failed: true })) === 'save-failed' &&
    status(view({ notify_failed: true, save_failed: true })) === 'save-failed');
  check('P3 a doorbell that could not be written is its own honest state and never 已生效',
    status(view({ notify_failed: true })) === 'notify-failed' &&
    statusText('notify-failed', '').indexOf('已生效') < 0);
  check('P4 a matching failure is reported with the extension\'s own reason; an older id is not',
    status(view({ result_id: 'r1', result_state: 'failed' })) === 'failed' &&
    status(view({ result_id: 'r0', result_state: 'failed' })) === 'pending' &&
    statusText('failed', 'route table rejected') === '已保存，但应用失败：route table rejected' &&
    statusText('failed', '') === '已保存，但应用失败。');
  check('P5 已生效 needs a MATCHING id, the applied predicate AND a live document',
    status(view({ result_id: 'r1', result_state: 'applied', result_applied: true })) === 'applied' &&
    statusText('applied', '') === '已生效。' &&
    status(view({ result_id: 'r0', result_state: 'applied', result_applied: true })) === 'pending' &&
    status(view({ result_id: 'r1', result_state: 'applying' })) === 'pending' &&
    status(view({ result_id: 'r1', result_state: 'applied', result_applied: true,
      has_live_document: false })) === 'next-connect');
  check('P6 a stored-no-map answer parses as applied but is worded as 下次连接',
    status(view({ result_id: 'r1', result_state: 'applied', result_applied: false })) === 'next-connect' &&
    statusText('next-connect', '') === '已保存，将在下次连接时生效。');
  check('P7 the two failure tones are errors, the in-progress ones warn, 已生效 is ok and idle is silent',
    statusTone('applied') === 'ok' && statusTone('failed') === 'error' &&
    statusTone('save-failed') === 'error' && statusTone('saving') === 'warn' &&
    statusTone('pending') === 'warn' && statusTone('next-connect') === 'warn' &&
    statusTone('idle') === '');
  check('P8 the policy takes the result state tokens from the IPC contract instead of re-declaring them',
    POLICY_SOURCE.indexOf('export function routeApplyStatus(') >= 0 &&
    POLICY_SOURCE.includes("from './NetBirdRouteRequest'") &&
    POLICY_SOURCE.includes('ROUTE_APPLY_STATE_APPLIED') &&
    POLICY_SOURCE.indexOf('ROUTE_RESULT_STATE') < 0 &&
    POLICY_SOURCE.indexOf('@ohos') < 0 && POLICY_SOURCE.indexOf('$r(') < 0);

  // ---- R: the request id ---------------------------------------------------
  const idOf = policy.routeApplyRequestId as (seq: number, now: number) => string;
  const ids: string[] = [idOf(1, 1700000000000), idOf(2, 1700000000000),
    idOf(1, 1700000000001), idOf(2, 1700000000001)];
  check('R1 every completed save gets a fresh, unique, opaque id: the counter breaks same-millisecond ties',
    idOf(1, 1700000000000) === 'ui-route-1700000000000-1' &&
    new Set(ids).size === ids.length && ids.every((id: string): boolean => id.startsWith('ui-route-')));

  // ---- H: one HA NetID is one row -----------------------------------------
  const rows = prefs.routeSelectionRows(
    { selected_routes: [], deselected_routes: [], deselect_all: false }, catalog) as any[];
  check('H1 the two HA candidates of one NetID are ONE row (never a route id/peer row)',
    rows.length === 2 && rows[0].net_id === 'HostInternal' && rows[0].candidate_count === 2 &&
    rows[0].networks.length === 1);
  check('H2 the row carries its subnet(s) and 节点数 through the pure policy text',
    (policy.routePrefixesText as (p: string[]) => string)(rows[0].networks) === '192.168.50.0/24' &&
    (policy.routeCandidateText as (n: number) => string)(rows[0].candidate_count) === '节点数 2');
  check('H3 the row name is the NetID verbatim; an absent NetID is said plainly',
    (policy.routeNetName as (n: string) => string)('HostInternal') === 'HostInternal' &&
    (policy.routeNetName as (n: string) => string)('') === '（管理端未提供网络标识）' &&
    (policy.routePrefixesText as (p: string[]) => string)([]) === '未提供网段');

  // ---- O: ONE switch action writes the preference BEFORE the doorbell ------
  stub.configuredUrl = MGMT;
  stub.published = { state: 'connected', request_id: 'req-live' };
  stub.routeCatalog = catalog;
  stub.routeDraft = prefs.routeSelectionDefault();
  renames.length = 0;
  writeCalls = 0;
  requestCalls = 0;
  maxInFlight = 0;

  stub.routeToggle('HostInternal');
  await settle();

  const firstRequest: any = await requestStore.readRouteApplyRequest(filesDir);
  check('O1 one switch action persists the NEW selection at once — no 保存 and no 继续 step',
    storedSelection() !== null && storedSelection().deselect_all === false &&
    storedSelection().deselected_routes.indexOf('HostInternal') >= 0);
  check('O2 the preference document is committed BEFORE the apply request: the order is on the rename record',
    renames.indexOf(`${SELECTION_FILE}.tmp`) >= 0 &&
    renames.indexOf(`${SELECTION_FILE}.tmp`) < renames.indexOf(`${REQUEST_FILE}.tmp`),
    JSON.stringify(renames));
  check('O3 the request is a pure notification: opaque id + saved scope URL + stamp, no selection copy, no session',
    firstRequest !== null && firstRequest.management_url === MGMT &&
    typeof firstRequest.id === 'string' && firstRequest.id.length > 0 &&
    deepEq(Object.keys(JSON.parse(requestText() as string)),
      ['v', 'id', 'management_url', 'created_at_ms']) &&
    !/selection|session|token|jwt|secret/i.test(requestText() as string));
  check('O4 the doorbell was rung exactly once, with the id of the save that just settled',
    requestCalls === 1 && firstRequest.id === stub.routeRequestId && stub.routeRequestSeq === 1);
  check('O5 the page writes through the real atomic store: no temp sibling survives',
    noTempResidue() && readdirSync(filesDir).indexOf(RESULT_FILE) < 0);

  // ---- S: fast toggles, latest wins ---------------------------------------
  stub.published = { state: 'connected', request_id: 'req-live' };
  stub.routeDraft = prefs.routeSelectionDefault();
  renames.length = 0;
  writeCalls = 0;
  requestCalls = 0;
  maxInFlight = 0;

  stub.routeToggle('HostInternal');   // wish A: this network off
  stub.routeDisableAll();             // wish B: everything off
  stub.routeEnableAll();              // wish C: everything on (the last wish)
  await settle();

  check('S1 never more than ONE preference write is in flight, and the last wish is the one on disk',
    maxInFlight === 1 && writeCalls >= 1 && writeCalls <= 3 &&
    deepEq(storedSelection(), prefs.routeSelectionDefault()));
  check('S2 a superseded wish is never announced: exactly one request for the settled burst, carrying the newest id',
    requestCalls === 1 && (await requestStore.readRouteApplyRequest(filesDir)).id === stub.routeRequestId);
  check('S3 the rapid burst leaves no `.tmp` sibling and no half-written preference',
    noTempResidue() && prefs.parseRouteSelectionDocument(prefText() as string) !== null);

  // ---- F: a failed preference write ---------------------------------------
  const prefBefore = prefText();
  const reqBefore = requestText();
  writeCalls = 0;
  requestCalls = 0;
  state.writeError = { code: 1, message: 'disk full' };
  stub.routeDraft = prefs.routeSelectionDefault();
  stub.routeSaveFailed = false;
  stub.routeToggle('HostInternal');
  await settle();

  check('F1 a FAILED preference write stores nothing, rings NO doorbell and leaves the connection untouched',
    requestCalls === 0 && stub.routeSaveFailed === true && inFlight === 0 &&
    prefText() === prefBefore && requestText() === reqBefore);
  check('F2 the page still shows the user\'s latest wish and the honest save-failure wording',
    prefs.routeSelectionIsEnabled(stub.routeDraft, 'HostInternal') === false &&
    stub.routeStatusText() === '保存失败，路由选择未生效；连接状态未改变。' &&
    stub.routeStatusTone() === 'error');
  state.writeError = null;
  requestCalls = 0;
  stub.routeToggle('HostInternal');
  await settle();
  check('F3 the page recovers on the next action: the write lands and the doorbell rings once more',
    requestCalls === 1 && stub.routeSaveFailed === false &&
    prefs.routeSelectionIsEnabled(storedSelection(), 'HostInternal') === true);

  // ---- G: the in-flight read cannot clobber a moved switch ---------------
  loadDeferred = true;
  stub.routeUserEdited = false;
  stub.routeDraft = prefs.routeSelectionDefault();
  stub.routeSelectionLoading = false;
  stub.loadRouteSelection();
  const wished: any = prefs.routeSelectionDeselectAll();
  stub.routeApplyAction(wished);
  (loadRelease as (value: any) => void)(prefs.routeSelectionDefault());
  await settle();
  loadDeferred = false;
  check('G1 a selection read that settles AFTER the user moved a switch never clobbers that switch',
    deepEq(stub.routeDraft, wished) && stub.routeUserEdited === true);
  await store.saveRouteSelection(filesDir, MGMT, prefs.routeSelectionDefault());
  stub.routeUserEdited = false;
  stub.routeDraft = prefs.routeSelectionDeselectAll();
  stub.loadRouteSelection();
  await settle();
  check('G2 with no user edit the same read does seed the page from the stored preference',
    deepEq(stub.routeDraft, prefs.routeSelectionDefault()) && stub.routeLoadError === false);

  // ---- Q: the result document --------------------------------------------
  stub.published = { state: 'connected', request_id: 'req-live' };
  stub.routeEnableAll();
  await settle();
  const liveId: string = stub.routeRequestId;
  check('Q1 a fresh request resets the answer slates: the live page says 正在应用… and never 已生效',
    stub.routeResultId === '' && stub.routeResultApplied === false &&
    stub.routeStatusText() === '已保存，正在应用…');

  await requestStore.writeRouteApplyResult(filesDir, { id: 'ui-route-1-1', state: 'applied',
    mode: 'recreated', error: '', routes: ['HostInternal'], at_ms: Date.now() });
  stub.refreshRouteApplyResult();
  await settle();
  check('Q2 an OLDER generation\'s applied answer is ignored entirely',
    stub.routeStatusText() === '已保存，正在应用…' && stub.routeResultApplied === false);

  await requestStore.writeRouteApplyResult(filesDir, { id: liveId, state: 'applying',
    mode: '', error: '', routes: [], at_ms: Date.now() });
  stub.refreshRouteApplyResult();
  await settle();
  check('Q3 a matching `applying` answer is still not 已生效',
    stub.routeStatusText() === '已保存，正在应用…' && stub.routeResultApplied === false);

  await requestStore.writeRouteApplyResult(filesDir, { id: liveId, state: 'applied',
    mode: 'recreated', error: '', routes: ['HostInternal'], at_ms: Date.now() });
  stub.refreshRouteApplyResult();
  await settle();
  check('Q4 an existing matching applied answer is shown as 已生效',
    stub.routeStatusText() === '已生效。' && stub.routeStatusTone() === 'ok' &&
    stub.routeResultApplied === true);

  await requestStore.writeRouteApplyResult(filesDir, { id: liveId, state: 'applied',
    mode: 'stored-no-map', error: '', routes: [], at_ms: Date.now() });
  stub.refreshRouteApplyResult();
  await settle();
  check('Q5 a stored-no-map answer is NOT 已生效: the preference is stored but no map reached the OS',
    stub.routeResultApplied === false && stub.routeStatusText() === '已保存，将在下次连接时生效。');

  await requestStore.writeRouteApplyResult(filesDir, { id: liveId, state: 'failed',
    mode: '', error: 'route table rejected', routes: [], at_ms: Date.now() });
  stub.refreshRouteApplyResult();
  await settle();
  check('Q6 a matching failure is reported honestly, with the extension\'s own reason',
    stub.routeStatusText() === '已保存，但应用失败：route table rejected' &&
    stub.routeStatusTone() === 'error');

  stub.routeResultId = '';
  stub.routeResultState = '';
  stub.routeResultApplied = false;
  stub.routeDraft = prefs.routeSelectionDeselectAll();
  check('Q7 the page\'s enabled verdicts come from the selection, never from the catalog\'s own enabled column',
    stub.routeSummaryText() === '已启用 0/2' &&
    catalog.networks.every((entry: any): boolean => entry.enabled === true));

  // ---- L: offline ---------------------------------------------------------
  stub.published = null;
  stub.routeDraft = prefs.routeSelectionDefault();
  requestCalls = 0;
  stub.routeToggle('HostInternal');
  await settle();
  check('L1 offline the preference AND the notification are still written, and the wording is 下次连接时生效',
    storedSelection().deselected_routes.indexOf('HostInternal') >= 0 && requestCalls === 1 &&
    (await requestStore.readRouteApplyRequest(filesDir)) !== null &&
    stub.routeStatusText() === '已保存，将在下次连接时生效。');
  await requestStore.writeRouteApplyResult(filesDir, { id: stub.routeRequestId, state: 'applied',
    mode: 'recreated', error: '', routes: ['HostInternal'], at_ms: Date.now() });
  stub.refreshRouteApplyResult();
  await settle();
  check('L2 offline never claims 已生效, even with a matching applied answer on disk',
    stub.routeStatusText() === '已保存，将在下次连接时生效。' && stub.routeResultApplied === true);

  stub.published = { state: 'connected', request_id: 'req-live' };
  notifyFail = true;
  requestCalls = 0;
  stub.routeEnableAll();
  await settle();
  check('L3 a saved preference whose doorbell could not be written is reported as such, never as 已生效',
    requestCalls === 1 && stub.routeNotifyFailed === true &&
    stub.routeStatusText() === '已保存；未能通知当前连接，将在下次连接时生效。' &&
    deepEq(storedSelection(), prefs.routeSelectionDefault()));
  notifyFail = false;

  // ---- T: the row switch over the REAL onChange body ---------------------
  stub.published = { state: 'connected', request_id: 'req-live' };
  stub.routeCatalog = catalog;
  stub.routeLoadError = false;

  stub.routeDraft = prefs.routeSelectionDefault();
  writeCalls = 0;
  requestCalls = 0;
  stub.routeToggleChange(true, { net_id: 'HostInternal' });
  await settle();
  check('T1 an all-enabled selection re-emitting isOn=true changes nothing (the guard runs before any flip)',
    writeCalls === 0 && requestCalls === 0 && prefs.routeSelectionIsEnabled(stub.routeDraft, 'HostInternal'));

  stub.routeDraft = prefs.routeSelectionDeselectAll();
  writeCalls = 0;
  stub.routeToggleChange(false, { net_id: 'HostInternal' });
  await settle();
  check('T2 an all-disabled selection re-emitting isOn=false changes nothing either',
    writeCalls === 0 && !prefs.routeSelectionIsEnabled(stub.routeDraft, 'HostInternal') &&
    stub.routeDraft.deselect_all === true);

  stub.routeDraft = prefs.routeSelectionDefault();
  writeCalls = 0;
  requestCalls = 0;
  stub.routeToggleChange(false, { net_id: 'HostInternal' });
  stub.routeToggleChange(false, { net_id: 'HostInternal' });
  await settle();
  check('T3 a REAL change flips exactly once and the programmatic duplicate callback is idempotent',
    writeCalls === 1 && requestCalls === 1 &&
    !prefs.routeSelectionIsEnabled(stub.routeDraft, 'HostInternal'));

  stub.routeDraft = prefs.routeSelectionDeselectAll();
  writeCalls = 0;
  requestCalls = 0;
  stub.routeToggleChange(true, { net_id: 'HostInternal' });
  stub.routeToggleChange(true, { net_id: 'HostInternal' });
  await settle();
  check('T4 a real change the other way flips once too: enabling under the global all-off is the explicit exception',
    writeCalls === 1 && requestCalls === 1 &&
    prefs.routeSelectionIsEnabled(stub.routeDraft, 'HostInternal') &&
    stub.routeDraft.deselect_all === false);

  // ---- A: the two global verbs -------------------------------------------
  writeCalls = 0;
  requestCalls = 0;
  stub.routeDisableAll();
  await settle();
  check('A1 全部停用 writes through at once and keeps its GLOBAL meaning (future routes included)',
    storedSelection().deselect_all === true && requestCalls === 1 && writeCalls === 1);

  writeCalls = 0;
  requestCalls = 0;
  stub.routeEnableAll();
  await settle();
  check('A2 全部启用 writes through at once and clears the flag so future routes are on again',
    deepEq(storedSelection(), prefs.routeSelectionDefault()) && requestCalls === 1);

  const routeView = sliceFrom(PAGE, '@Builder\n  routeSelectionView() {',
    'private openRouteSelection(): void');
  const routeRow = sliceFrom(PAGE, '@Builder\n  routeRow(entry: RouteNetworkEntry) {',
    '@Builder\n  routeSelectionView()');
  check('A3 neither a switch nor a global button is gated by the in-flight write (only an unreadable record disables them)',
    routeRow.includes('.enabled(!this.routeLoadError)') &&
    routeView.includes('.enabled(!this.routeLoadError)') &&
    routeView.indexOf('routeApplyBusy') < 0 && routeRow.indexOf('routeApplyBusy') < 0);

  // ---- N: no stop / start / re-login path anywhere -----------------------
  const routePage = sliceFrom(PAGE, 'private routeApplyAction(selection: RouteSelection): void',
    'private closeVisibleSubflow(): boolean');
  const closeHandler = sliceFrom(PAGE, 'private closeVisibleSubflow(): boolean', 'onBackPress(): boolean');
  check('N1 the whole route page path contains no stop, start or re-login call at all',
    routePage.indexOf('stopVpnExtensionAbility') < 0 &&
    routePage.indexOf('startVpnExtensionAbility') < 0 &&
    routePage.indexOf('requestStart') < 0 && routePage.indexOf('requestStop') < 0 &&
    routePage.indexOf('buildWant') < 0 && routePage.indexOf('vpnExtension') < 0);
  check('N2 the retired save/stop/ACK machinery is gone from the page',
    ['ROUTE_ACK_', 'routeStopThenStart', 'routeConfirmedStop', 'routeStartNow',
      'applyRouteSelection', 'routeAfterSave', 'readRouteStopAck', 'discardRouteDraft',
      'ROUTE_MANUAL_NOTICE', 'STOP_ACK_FILE_NAME', 'NetBirdStopAck'].every(
      (token: string): boolean => PAGE.indexOf(token) < 0));
  check('N3 no timer was added for the increment: the page still has exactly its one poll interval',
    PAGE.split('setInterval(').length === 2);
  check('N4 the page reaches the live-route pair only through the routes/ stores, never a file path of its own',
    PAGE.indexOf(REQUEST_FILE) < 0 && PAGE.indexOf(RESULT_FILE) < 0 &&
    PAGE.indexOf("from '../routes/NetBirdRouteRequestStore'") >= 0 &&
    PAGE.indexOf("from '../routes/NetBirdRouteRequest'") >= 0);
  check('N5 the policy no longer imports the stop-ack module: the stop leg went with the ACK wait',
    POLICY_SOURCE.indexOf("from '../vpnextensionability/NetBirdStopAck'") < 0 &&
    POLICY_SOURCE.indexOf('stopAckReadVerdict') < 0);
  check('N6 every route write/read is scoped to the SAVED management URL, never the unsaved editor field',
    routePage.includes('this.configuredUrl') && sliceFrom(PAGE,
      'private loadRouteSelection(): void', 'private refreshRouteApplyResult(): void')
      .split('this.urlInput').length === 1 && routePage.indexOf('urlInput') < 0);
  check('N7 a catalog refresh never touches the latest desired selection (a refresh cannot discard a change)',
    sliceFrom(PAGE, 'private refreshRouteCatalog(): void', 'private refreshRouteApplyResult(): void')
      .indexOf('this.routeDraft') < 0 &&
    sliceFrom(PAGE, 'private refreshRouteApplyResult(): void', 'private routeRows(): RouteNetworkEntry[]')
      .indexOf('this.routeDraft') < 0);

  // ---- W: wording, back and theme ----------------------------------------
  check('W1 the page states the change is immediate and that switching can briefly interrupt data (no zero-gap claim)',
    routeView.includes('改动立即保存并通知当前连接') &&
    routeView.includes('切换瞬间可能短暂中断数据') &&
    routeView.indexOf('不中断') < 0 && routeView.indexOf('无感知') < 0);
  check('W2 no 保存 / 放弃更改 button remains on the route page',
    routeView.indexOf("Button('放弃更改')") < 0 && routeView.indexOf("Button('保存") < 0 &&
    routeView.indexOf('Button(this.routeApplyLabel())') < 0 &&
    PAGE.indexOf('routeApplyLabel') < 0 && PAGE.indexOf('routeApplyEnabled') < 0 &&
    PAGE.indexOf('UI_ROUTE_APPLY_CLICKED') < 0);
  check('W3 back returns to 设置 with nothing to discard and no connection operation',
    closeHandler.includes('this.routeSelectionOpen = false;') &&
    closeHandler.indexOf('discardRouteDraft') < 0 &&
    closeHandler.indexOf('stopVpnExtensionAbility') < 0 &&
    closeHandler.indexOf('requestStart') < 0 && closeHandler.indexOf('routeApplyBusy') < 0 &&
    closeHandler.includes('this.activeTab === 2 && !this.showDiagnostics && this.routeSelectionOpen'));
  check('W4 the global all-off note and the plain-Chinese catalog/empty states are unchanged',
    PAGE.includes("Text('全部停用也适用于以后新增的路由')") &&
    PAGE.includes("this.sectionTitle('路由选择')") &&
    sliceFrom(PAGE, 'private routeCatalogNote(): string', 'private routeEmptyText(): string')
      .includes('上次连接获取') &&
    sliceFrom(PAGE, 'private routeEmptyText(): string', 'private routeNoRowsText(): string')
      .includes('尚无路由目录'));
  check('W5 every route-page colour is still a palette resource: no hardcoded hex, no sourceColor change',
    routeView.includes("$r('app.color.") && routeRow.includes("$r('app.color.") &&
    routeView.match(/'#[0-9A-Fa-f]{3,8}'/) === null &&
    routeRow.match(/'#[0-9A-Fa-f]{3,8}'/) === null &&
    routeView.indexOf('TextInput') < 0);
  check('W6 ordinary and exit entries stay separate sections, keyed by NetID only, and the isOn guard is textual',
    routeView.includes("this.sectionTitle('普通路由')") &&
    routeView.includes("this.sectionTitle('出口节点')") &&
    routeRow.includes('routeSelectionIsEnabled(this.routeDraft, entry.net_id)') &&
    routeRow.includes('if (isOn !== routeSelectionIsEnabled(this.routeDraft, entry.net_id)) {') &&
    routeRow.indexOf('_isOn') < 0 && routeRow.indexOf('candidate_count') >= 0 &&
    routeRow.indexOf('route_id') < 0 && routeRow.indexOf('peer') < 0);

  // ---- E: 返回后重新进入 (re-entry) while a read or a write is still in flight -
  //
  // Every sequence below is driven through the REAL openRouteSelection() and
  // loadRouteSelection() with the store promises HELD open, so the order in
  // which an old answer and the user's new wish settle is chosen by the test
  // rather than by the machine. The same sequences are then run once against
  // the throwaway pre-fix fixture: the negative control must show the bug.
  const ALL_ON: any = prefs.routeSelectionDefault();
  const ALL_OFF: any = prefs.routeSelectionDeselectAll();
  const appliedResult = (id: string): any => ({
    id: id, state: 'applied', mode: 'recreated', error: '', routes: ['HostInternal'],
    at_ms: Date.now()
  });
  const resetPage = (p: any): void => {
    p.configuredUrl = MGMT;
    p.published = { state: 'connected', request_id: 'req-live' };
    p.routeCatalog = catalog;
    p.routeDraft = prefs.routeSelectionClone(ALL_ON);
    p.routeSelectionOpen = false;
    p.routeUserEdited = false;
    p.routeSelectionLoading = false;
    p.routeWriteBusy = false;
    p.routeWritePending = null;
    p.routeSaving = false;
    p.routeSaveFailed = false;
    p.routeNotifyFailed = false;
    p.routeActionError = '';
    p.routeLoadError = false;
    p.routeLoadErrorText = '';
    p.routeCatalogReading = false;
    p.routeResultReading = false;
    p.routeRequestId = '';
    p.routeResultId = '';
    p.routeResultState = '';
    p.routeResultApplied = false;
    p.routeResultError = '';
  };
  // The one shipped statement of the route page's back handler (closeVisibleSubflow's
  // `close-route` branch, pinned textually by W3 of this harness and by
  // run-navigation-back.mts): back leaves the write in flight and returns to 设置.
  const backOut = (p: any): void => { p.routeSelectionOpen = false; };

  // Sequence 1: the user presses 全部停用 (wish B), its write is held in flight,
  // the user backs out and walks straight back in while the disk still holds the
  // older all-on document. Returns how many times re-entry asked the disk.
  const writePendingReopen = async (p: any): Promise<number> => {
    resetPage(p);
    await store.saveRouteSelection(filesDir, MGMT, ALL_ON);
    p.openRouteSelection();
    await settle(p);
    if (!deepEq(p.routeDraft, ALL_ON)) {
      throw new Error('sequence 1: the first entry did not seed from the stored document');
    }
    loadCalls = 0;
    loadRelease = null;
    saveRelease = null;
    loadDeferred = true;
    saveDeferred = true;
    p.routeDisableAll();
    backOut(p);
    p.openRouteSelection();
    const asked: number = loadCalls;
    if (loadRelease !== null) {
      // a read WAS asked while the page's own write was in flight: answer it with
      // the older all-on document
      const answer: any = loadRelease;
      loadRelease = null;
      answer(prefs.routeSelectionClone(ALL_ON));
    }
    // only now does the held write of 全部停用 reach the disk and ring the doorbell
    (saveRelease as () => void)();
    saveDeferred = false;
    loadDeferred = false;
    await settle(p);
    await requestStore.writeRouteApplyResult(filesDir, appliedResult(p.routeRequestId));
    p.refreshRouteApplyResult();
    await settle(p);
    return asked;
  };

  // Sequence 2: the first read is still in flight when the user moves a switch;
  // the wish's own write settles, then the user backs out and back in, and only
  // then does that first read answer with the older document.
  const initialLoadPendingReopen = async (p: any): Promise<void> => {
    resetPage(p);
    await store.saveRouteSelection(filesDir, MGMT, ALL_ON);
    loadCalls = 0;
    loadRelease = null;
    loadDeferred = true;
    p.openRouteSelection();
    if (loadRelease === null || p.routeSelectionLoading !== true) {
      throw new Error('sequence 2: the first selection read was not issued');
    }
    p.routeDisableAll();
    for (let i = 0; i < 1000 && (p.routeWriteBusy || p.routeWritePending !== null); i++) {
      await tick();
    }
    backOut(p);
    p.openRouteSelection();
    const answer: any = loadRelease;
    loadRelease = null;
    loadDeferred = false;
    answer(prefs.routeSelectionClone(ALL_ON));
    await settle(p);
    saveDeferred = false;
  };

  // Sequence 3: the preference is saved but the doorbell could not be written;
  // the user backs out and back in.
  const notifyFailureReopen = async (p: any): Promise<void> => {
    resetPage(p);
    await store.saveRouteSelection(filesDir, MGMT, ALL_ON);
    p.openRouteSelection();
    await settle(p);
    notifyFail = true;
    p.routeEnableAll();
    await settle(p);
    notifyFail = false;
    if (p.routeNotifyFailed !== true) {
      throw new Error('sequence 3: the failed doorbell was not recorded');
    }
    backOut(p);
    p.openRouteSelection();
    await settle(p);
  };

  // Sequence 4: the newest request answers 已生效, and the NEXT wish's save is
  // then held in flight. Returns the older request id.
  const saveOverOldApplied = async (p: any): Promise<string> => {
    resetPage(p);
    await store.saveRouteSelection(filesDir, MGMT, ALL_ON);
    p.openRouteSelection();
    await settle(p);
    p.routeEnableAll();
    await settle(p);
    const previousId: string = p.routeRequestId;
    await requestStore.writeRouteApplyResult(filesDir, appliedResult(previousId));
    p.refreshRouteApplyResult();
    await settle(p);
    if (p.routeStatusText() !== '已生效。') {
      throw new Error('sequence 4: the newer applied answer was not adopted');
    }
    saveRelease = null;
    saveDeferred = true;
    p.routeDisableAll();
    return previousId;
  };

  const askedOnReentry: number = await writePendingReopen(stub);
  check('E1 re-entry while the wish\'s own write is in flight does not even ask the disk for an older answer',
    askedOnReentry === 0 && deepEq(stub.routeDraft, ALL_OFF) &&
    prefs.routeSelectionIsEnabled(stub.routeDraft, 'HostInternal') === false &&
    stub.routeUserEdited === true);
  check('E2 the held write lands afterwards and the switch, the disk and the 已生效 answer all agree on 全部停用',
    deepEq(storedSelection(), ALL_OFF) && deepEq(stub.routeDraft, ALL_OFF) &&
    stub.routeStatusText() === '已生效。' && stub.routeStatusTone() === 'ok' &&
    (await requestStore.readRouteApplyRequest(filesDir)).id === stub.routeRequestId);

  await initialLoadPendingReopen(stub);
  check('E3 a read that was already in flight when the switch moved is discarded after re-entry too',
    deepEq(stub.routeDraft, ALL_OFF) && stub.routeUserEdited === true &&
    prefs.routeSelectionIsEnabled(stub.routeDraft, 'HostInternal') === false &&
    stub.routeStatusText() === '已保存，正在应用…');
  check('E4 the wish itself is on disk and the doorbell carries its own id: the stale read changed nothing',
    deepEq(storedSelection(), ALL_OFF) &&
    (await requestStore.readRouteApplyRequest(filesDir)).id === stub.routeRequestId);

  await notifyFailureReopen(stub);
  check('E5 a doorbell that could not be written stays a failure across re-entry instead of becoming 正在应用 forever',
    stub.routeNotifyFailed === true &&
    stub.routeStatusText() === '已保存；未能通知当前连接，将在下次连接时生效。');

  resetPage(stub);
  await store.saveRouteSelection(filesDir, MGMT, ALL_ON);
  stub.routeDraft = prefs.routeSelectionClone(ALL_OFF);
  stub.openRouteSelection();
  await settle();
  check('E6 with nothing in flight the same re-entry still seeds the page from the stored document',
    deepEq(stub.routeDraft, ALL_ON) && stub.routeUserEdited === false);

  const previousId: string = await saveOverOldApplied(stub);
  check('E7 while the new save is in flight 正在保存 outranks the older answer, and that answer is NOT dropped early',
    stub.routeSaving === true && deepEq(stub.routeDraft, ALL_OFF) &&
    stub.routeRequestId === previousId && stub.routeResultId === previousId &&
    stub.routeResultApplied === true &&
    stub.routeStatusText() === '正在保存…' && stub.routeStatusTone() === 'warn');
  (saveRelease as () => void)();
  saveDeferred = false;
  await settle();
  check('E8 when that save lands the new request takes the slate and the older answer is gone',
    stub.routeRequestId !== previousId && stub.routeResultId === '' &&
    stub.routeResultApplied === false && stub.routeStatusText() === '已保存，正在应用…');

  // ---- E-negative: the throwaway pre-fix fixture must show every one of them --
  const oldAsked: number = await writePendingReopen(oldStub);
  check('E9 pre-fix control: the same sequence really did lose the wish on re-entry (stale read wins)',
    oldAsked === 1 && deepEq(oldStub.routeDraft, ALL_ON) &&
    oldStub.routeStatusText() === '已生效。');
  await initialLoadPendingReopen(oldStub);
  check('E10 pre-fix control: the same sequence really did adopt the stale read after re-entry',
    deepEq(oldStub.routeDraft, ALL_ON) && oldStub.routeUserEdited === false);
  await notifyFailureReopen(oldStub);
  check('E11 pre-fix control: the same sequence really did forget the failed doorbell on re-entry',
    oldStub.routeNotifyFailed === false &&
    oldStub.routeStatusText() === '已保存，正在应用…');
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  if (fixtureDir.length > 0) {
    rmSync(fixtureDir, { recursive: true, force: true });
  }
}

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
