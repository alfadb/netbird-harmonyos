/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side per-NetID route-selection checks (no device, no network, no
// compiler). Same principle as the other run-*.mts suites: the REAL production
// modules are copied byte-for-byte into an OS-temp fixture (.ts) at run time —
// the code under test is the shipped code, never a hand-held mirror:
//   NetBirdRoutePreferences.ets  (pure state + document parsing)
//   NetBirdRouteStore.ets        (atomic file IO; @ohos.file.fs / @ohos.util
//                                 are functional stubs over the real node fs)
//
// Coverage:
//   S  state: default all-on incl. future NetIDs, one HA group toggles off/on
//      by NetID, per-NetID isolation, fresh-state/no-mutation, unknown NetID
//      rejected atomically, all-off mode (incl. the explicit enable exception
//      and the disable no-op), select-all/deselect-all, deselected-wins
//      conflict, clone/equals draft helpers
//   D  documents: exact bytes, order-independent canonical bytes, dedupe,
//      missing keys, unknown keys, version errors, malformed shapes, NetID
//      preserved verbatim (empty NetID stays its own key)
//   C  catalog: native getter contract roundtrip, serial/candidate/exit
//      metadata, scope, stale timing, malformed shapes
//   F  file IO: missing file, roundtrip, other-server scope, malformed
//      existing file (stable error, file untouched), save validation before
//      write, read failure != missing, atomic failure retains the old file,
//      catalog write/read statuses, no credential fields, device config
//      untouched
//
// Run: node client/entry/selfcheck/run-route-preferences.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFile, writeFile, mkdtemp, rm, mkdir, open, rename, unlink, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./route-preferences-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const routesDir = join(here, '..', 'src', 'main', 'ets', 'routes');
const temp = await mkdtemp(join(tmpdir(), 'nb-route-prefs-'));
const fixture = join(temp, 'fixture');
await mkdir(fixture);
const filesDir = join(temp, 'files');
await mkdir(filesDir);

let passed = 0;
let failed = 0;

function check(name: string, ok: boolean): void {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}`);
  if (ok) { passed++; } else { failed++; }
}

function deepEq(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function reasonOf(run: () => unknown): string {
  try { run(); return ''; } catch (error) { return (error as { reason?: string }).reason ?? ''; }
}

async function reasonOfAsync(run: () => Promise<unknown>): Promise<string> {
  try { await run(); return ''; } catch (error) { return (error as { reason?: string }).reason ?? ''; }
}

async function exists(path: string): Promise<boolean> {
  try { await access(path); return true; } catch (error) { return false; }
}

const MGMT = 'https://management.example.com';
const MGMT_OTHER = 'https://other.example.com';
const HOST = 'HostInternal';
const CORP = 'corp';
const EXIT = 'exit-node';
const KNOWN: string[] = [HOST, CORP, EXIT];

const state: {
  nodeFs: { open: typeof open; rename: typeof rename; unlink: typeof unlink };
  readError: { code: number } | null;
  writeError: Error | null;
} = {
  nodeFs: { open, rename, unlink },
  readError: null,
  writeError: null
};
(globalThis as unknown as { __NB_ROUTE_TEST: typeof state }).__NB_ROUTE_TEST = state;

interface NetEntry {
  net_id: string;
  networks: string[];
  candidate_count: number;
  enabled: boolean;
  is_exit: boolean;
}

function entry(netId: string, prefixes: string[], candidates: number,
  enabled: boolean, isExit: boolean): NetEntry {
  return {
    net_id: netId,
    networks: prefixes,
    candidate_count: candidates,
    enabled: enabled,
    is_exit: isExit
  };
}

// The live HA shape from docs/route-audit.md: one NetID, two candidate rows,
// one prefix. Plus a plain subnet and an explicit IPv4 exit (0.0.0.0/0).
function catalogFixture(): {
  available: boolean; serial: number;
  selection: { selected_routes: string[]; deselected_routes: string[]; deselect_all: boolean };
  networks: NetEntry[];
} {
  return {
    available: true,
    serial: 302,
    selection: { selected_routes: [], deselected_routes: [], deselect_all: false },
    networks: [
      entry(HOST, ['192.168.50.0/24'], 2, true, false),
      entry(CORP, ['10.10.0.0/16'], 1, true, false),
      entry(EXIT, ['0.0.0.0/0'], 1, false, true)
    ]
  };
}

function selectionFixture(): { selected_routes: string[]; deselected_routes: string[]; deselect_all: boolean } {
  return { selected_routes: [HOST], deselected_routes: [CORP], deselect_all: false };
}

try {
  // ---- fixtures are byte-identical -----------------------------------------
  const names = ['NetBirdRoutePreferences.ets', 'NetBirdRouteStore.ets'];
  for (const name of names) {
    const source = join(routesDir, name);
    await writeFile(join(fixture, name.replace('.ets', '.ts')), await readFile(source));
    check(`${name} fixture is byte-identical`,
      (await readFile(source)).equals(await readFile(join(fixture, name.replace('.ets', '.ts')))));
  }

  const prefs = await import(pathToFileURL(join(fixture, 'NetBirdRoutePreferences.ts')).href) as Record<string, any>;
  const store = await import(pathToFileURL(join(fixture, 'NetBirdRouteStore.ts')).href) as Record<string, any>;

  // ---- exported contract ---------------------------------------------------
  const stateFns = ['routeSelectionDefault', 'routeSelectionClone', 'routeSelectionEquals',
    'routeSelectionIsEnabled', 'routeSelectionHasUserSelection', 'routeSelectionSelectAll',
    'routeSelectionDeselectAll', 'routeSelectionEnable', 'routeSelectionDisable',
    'routeSelectionToggle', 'routeSelectionKnownNetIds', 'routeSelectionRows'];
  const documentFns = ['serializeRouteSelectionDocument', 'parseRouteSelectionDocument',
    'serializeRouteCatalogDocument', 'parseRouteCatalogDocument', 'routeCatalogIsStale'];
  check('pure module exports every state and document helper',
    stateFns.concat(documentFns).every((name) => typeof prefs[name] === 'function'));
  check('store exports the exact four async entry points',
    typeof store.loadRouteSelection === 'function' && typeof store.saveRouteSelection === 'function' &&
    typeof store.readRouteCatalog === 'function' && typeof store.writeRouteCatalog === 'function');
  check('document versions are pinned to 1',
    prefs.ROUTE_SELECTION_VERSION === 1 && prefs.ROUTE_CATALOG_VERSION === 1);
  check('file names and byte bounds are exact',
    store.ROUTE_SELECTION_FILE === 'netbird-route-selection.json' &&
    store.ROUTE_CATALOG_FILE === 'netbird-routes.json' &&
    store.MAX_SELECTION_DOCUMENT_BYTES === 64 * 1024 && store.MAX_CATALOG_DOCUMENT_BYTES === 512 * 1024);
  check('catalog status and failure tokens are the stable set',
    prefs.CATALOG_STATUS_OK === 'ok' && prefs.CATALOG_STATUS_MISSING === 'missing' &&
    prefs.CATALOG_STATUS_SCOPE_MISMATCH === 'scope-mismatch' &&
    prefs.CATALOG_STATUS_MALFORMED === 'malformed' && prefs.CATALOG_STATUS_VERSION === 'version' &&
    prefs.CATALOG_STATUS_UNAVAILABLE === 'unavailable' && prefs.REASON_UNKNOWN_ROUTE === 'unknown-route');

  // ---- state ---------------------------------------------------------------
  const fresh = prefs.routeSelectionDefault();
  check('default state is all-on with empty lists and the flag off',
    fresh.deselect_all === false && fresh.selected_routes.length === 0 &&
    fresh.deselected_routes.length === 0 && prefs.routeSelectionIsEnabled(fresh, HOST));
  check('default enables a NetID that the map does not contain yet',
    prefs.routeSelectionIsEnabled(fresh, 'appears-later'));
  const freshSnapshot = JSON.stringify(fresh);

  const off = prefs.routeSelectionDisable(fresh, HOST, KNOWN);
  check('disable records the HA group NetID, not a route id',
    deepEq(off.deselected_routes, [HOST]) && off.selected_routes.length === 0 && !off.deselect_all);
  check('disable turns the whole HA group off while siblings stay on',
    !prefs.routeSelectionIsEnabled(off, HOST) && prefs.routeSelectionIsEnabled(off, CORP) &&
    prefs.routeSelectionIsEnabled(off, EXIT));
  check('mutators return fresh objects and never mutate their input',
    off !== fresh && JSON.stringify(fresh) === freshSnapshot &&
    JSON.stringify(off) !== freshSnapshot);

  const back = prefs.routeSelectionEnable(off, HOST, KNOWN);
  check('enable returns the group to default-on and drops the deselection',
    prefs.routeSelectionIsEnabled(back, HOST) && deepEq(back.deselected_routes, []) &&
    deepEq(back.selected_routes, [HOST]));
  check('disable on one NetID never disturbs another', deepEq(
    prefs.routeSelectionDisable(fresh, CORP, KNOWN).deselected_routes, [CORP]) &&
    prefs.routeSelectionIsEnabled(prefs.routeSelectionDisable(fresh, CORP, KNOWN), HOST));

  const toggledOff = prefs.routeSelectionToggle(fresh, CORP, KNOWN);
  const toggledOn = prefs.routeSelectionToggle(toggledOff, CORP, KNOWN);
  check('toggle flips a NetID off and back on',
    !prefs.routeSelectionIsEnabled(toggledOff, CORP) && deepEq(toggledOff.selected_routes, []) &&
    prefs.routeSelectionIsEnabled(toggledOn, CORP) && deepEq(toggledOn.deselected_routes, []));
  check('toggle of an unknown NetID is rejected like an explicit enable',
    reasonOf(() => prefs.routeSelectionToggle(fresh, 'missing', KNOWN)) === 'unknown-route');

  const rejectedState = prefs.routeSelectionDisable(fresh, CORP, KNOWN);
  const rejectedSnapshot = JSON.stringify(rejectedState);
  let unknownError: { reason?: string; net_ids?: string[] } = {};
  try { prefs.routeSelectionEnable(rejectedState, 'missing', KNOWN); } catch (error) {
    unknownError = error as { reason?: string; net_ids?: string[] };
  }
  check('unknown NetID enable is rejected with the reason and the rejected id',
    unknownError.reason === 'unknown-route' && deepEq(unknownError.net_ids, ['missing']));
  check('a rejected mutation leaves the state byte-identical',
    JSON.stringify(rejectedState) === rejectedSnapshot);
  check('unknown NetID disable is rejected as well',
    reasonOf(() => prefs.routeSelectionDisable(fresh, 'missing', KNOWN)) === 'unknown-route');

  const allOff = prefs.routeSelectionDeselectAll();
  check('global all-off clears both lists and sets the flag',
    allOff.deselect_all === true && allOff.selected_routes.length === 0 &&
    allOff.deselected_routes.length === 0);
  check('global all-off blocks known and future NetIDs',
    !prefs.routeSelectionIsEnabled(allOff, HOST) && !prefs.routeSelectionIsEnabled(allOff, 'future'));

  const exception = prefs.routeSelectionEnable(allOff, HOST, KNOWN);
  check('enabling under all-off clears the flag', exception.deselect_all === false);
  check('enabling under all-off marks every known NetID deselected except the request',
    deepEq(exception.deselected_routes, [CORP, EXIT]) && deepEq(exception.selected_routes, [HOST]));
  check('enabling under all-off leaves future NetIDs default-on',
    prefs.routeSelectionIsEnabled(exception, 'future'));
  check('disable under all-off is a no-op even for an unknown NetID',
    deepEq(prefs.routeSelectionDisable(allOff, 'missing', KNOWN), allOff));
  check('unknown enable under all-off is rejected and keeps the flag',
    reasonOf(() => prefs.routeSelectionEnable(allOff, 'missing', KNOWN)) === 'unknown-route' &&
    allOff.deselect_all === true);

  const allOn = prefs.routeSelectionSelectAll();
  check('global all-on clears both lists and the flag',
    allOn.deselect_all === false && allOn.selected_routes.length === 0 &&
    allOn.deselected_routes.length === 0);
  check('global all-on re-enables a previously deselected NetID',
    prefs.routeSelectionIsEnabled(prefs.routeSelectionSelectAll(), CORP));
  check('deselect-all ignores the previous state entirely',
    deepEq(prefs.routeSelectionDeselectAll(), allOff));

  const conflictText = '{"v":1,"management_url":"' + MGMT +
    '","selection":{"selected_routes":["x"],"deselected_routes":["x"],"deselect_all":false}}';
  const conflict = prefs.parseRouteSelectionDocument(conflictText).selection;
  check('a hand-written selected+deselected conflict resolves as deselected wins',
    !prefs.routeSelectionIsEnabled(conflict, 'x') && prefs.routeSelectionIsEnabled(conflict, 'y'));
  check('the conflicting NetID still counts as explicit user intent',
    prefs.routeSelectionHasUserSelection(conflict, 'x') &&
    !prefs.routeSelectionHasUserSelection(fresh, 'x'));
  check('the conflict survives a reload verbatim',
    deepEq(prefs.parseRouteSelectionDocument(
      prefs.serializeRouteSelectionDocument(MGMT, conflict)).selection, conflict));

  const draft = prefs.routeSelectionClone(off);
  check('clone equals its source but shares no array identity',
    prefs.routeSelectionEquals(draft, off) && draft !== off &&
    draft.deselected_routes !== off.deselected_routes);
  const draftEdited = prefs.routeSelectionDisable(draft, CORP, KNOWN);
  check('editing a draft leaves the original untouched and is detectable by equals',
    prefs.routeSelectionEquals(draft, off) && !prefs.routeSelectionEquals(draftEdited, off));
  check('equals compares content, not identity',
    prefs.routeSelectionEquals(prefs.routeSelectionDefault(), prefs.routeSelectionDefault()) &&
    !prefs.routeSelectionEquals(off, back));

  // ---- selection document --------------------------------------------------
  const exactSelection = '{"v":1,"management_url":"' + MGMT +
    '","selection":{"selected_routes":["lan"],"deselected_routes":["corp"],"deselect_all":false}}';
  const serializedSelection = prefs.serializeRouteSelectionDocument(MGMT,
    { selected_routes: ['lan'], deselected_routes: ['corp'], deselect_all: false });
  check('selection document bytes are exact and key-ordered', serializedSelection === exactSelection);
  check('selection roundtrip is byte-stable',
    prefs.serializeRouteSelectionDocument(MGMT,
      prefs.parseRouteSelectionDocument(exactSelection).selection) === exactSelection);
  check('parsed NetID arrays are deduplicated and sorted',
    deepEq(prefs.parseRouteSelectionDocument('{"v":1,"management_url":"' + MGMT +
      '","selection":{"selected_routes":["b","a","a"],"deselected_routes":[],"deselect_all":false}}')
      .selection.selected_routes, ['a', 'b']));
  check('serialized bytes are independent of mutation/input order',
    prefs.serializeRouteSelectionDocument(MGMT,
      { selected_routes: ['b', 'a', 'a'], deselected_routes: [], deselect_all: false }) ===
    prefs.serializeRouteSelectionDocument(MGMT,
      { selected_routes: ['a', 'b'], deselected_routes: [], deselect_all: false }));
  check('missing selection keys become empty/false and unknown keys are ignored',
    deepEq(prefs.parseRouteSelectionDocument('{"v":1,"management_url":"' + MGMT +
      '","selection":{"future_field":7}}').selection, prefs.routeSelectionDefault()));
  check('a document with another management URL still parses (scope is the store\'s job)',
    prefs.parseRouteSelectionDocument('{"v":1,"management_url":"' + MGMT_OTHER +
      '","selection":{}}').management_url === MGMT_OTHER);

  check('version 2 is a stable version error',
    reasonOf(() => prefs.parseRouteSelectionDocument('{"v":2,"management_url":"' + MGMT +
      '","selection":{}}')) === 'version');
  check('a missing version is a stable version error',
    reasonOf(() => prefs.parseRouteSelectionDocument('{"management_url":"' + MGMT +
      '","selection":{}}')) === 'version');
  const malformedSelectionDocs: string[] = [
    '', 'not-json', 'null', '[]', '"x"',
    '{"v":1}',
    '{"v":1,"management_url":"' + MGMT + '"}',
    '{"v":1,"management_url":"' + MGMT + '","selection":null}',
    '{"v":1,"management_url":"' + MGMT + '","selection":[]}',
    '{"v":1,"management_url":"' + MGMT + '","selection":{"selected_routes":{}}}',
    '{"v":1,"management_url":"' + MGMT + '","selection":{"selected_routes":[1]}}',
    '{"v":1,"management_url":"' + MGMT + '","selection":{"selected_routes":null}}',
    '{"v":1,"management_url":"' + MGMT + '","selection":{"deselect_all":1}}',
    '{"v":1,"management_url":"' + MGMT + '","selection":{"deselect_all":null}}',
    '{"v":1,"management_url":"","selection":{}}',
    '{"v":1,"management_url":"bad url","selection":{}}',
    '{"v":1,"management_url":5,"selection":{}}',
    '{"v":1,"management_url":"' + MGMT + '","selection":{"selected_routes":["' +
      'A'.repeat(prefs.MAX_NET_ID_CHARS + 1) + '"]}}'
  ];
  check('every malformed selection document is a stable malformed error',
    malformedSelectionDocs.every((text) =>
      reasonOf(() => prefs.parseRouteSelectionDocument(text)) === 'malformed'));
  check('an invalid selection never serializes (save must fail before file IO)',
    reasonOf(() => prefs.serializeRouteSelectionDocument(MGMT,
      { selected_routes: ['a'], deselected_routes: [], deselect_all: 'no' })) === 'malformed' &&
    reasonOf(() => prefs.serializeRouteSelectionDocument('', prefs.routeSelectionDefault())) === 'malformed');

  // ---- catalog document ----------------------------------------------------
  const catalogText = prefs.serializeRouteCatalogDocument(MGMT, catalogFixture(), 'req-1', 1700000000000);
  const catalogDoc = prefs.parseRouteCatalogDocument(catalogText);
  check('catalog document bytes are exact and key-ordered',
    prefs.serializeRouteCatalogDocument(MGMT, catalogDoc.catalog, catalogDoc.request_id,
      catalogDoc.updated_at_ms) === catalogText);
  check('catalog keeps the wrapper scope, stamp and request id',
    catalogDoc.management_url === MGMT && catalogDoc.updated_at_ms === 1700000000000 &&
    catalogDoc.request_id === 'req-1');
  check('catalog keeps the native serial and the HA grouping',
    catalogDoc.catalog.serial === 302 && catalogDoc.catalog.available === true &&
    catalogDoc.catalog.networks[0].net_id === HOST &&
    catalogDoc.catalog.networks[0].candidate_count === 2 &&
    deepEq(catalogDoc.catalog.networks[0].networks, ['192.168.50.0/24']));
  check('catalog keeps the explicit IPv4 exit metadata',
    catalogDoc.catalog.networks[2].is_exit === true &&
    deepEq(catalogDoc.catalog.networks[2].networks, ['0.0.0.0/0']));
  check('minimal catalog wrapper bytes are exact',
    prefs.serializeRouteCatalogDocument(MGMT,
      { available: false, serial: 0, selection: prefs.routeSelectionDefault(), networks: [] }, 'r', 5) ===
    '{"v":1,"management_url":"' + MGMT + '","updated_at_ms":5,"request_id":"r","catalog":' +
    '{"available":false,"serial":0,"selection":{"selected_routes":[],"deselected_routes":[],' +
    '"deselect_all":false},"networks":[]}}');
  check('duplicate prefixes collapse in first-seen order',
    deepEq(prefs.parseRouteCatalogDocument(prefs.serializeRouteCatalogDocument(MGMT,
      { available: true, serial: 1, selection: prefs.routeSelectionDefault(),
        networks: [entry('ha', ['192.168.50.0/24', '192.168.50.0/24', '10.0.0.0/8'], 2, true, false)] },
      'r', 9)).catalog.networks[0].networks, ['192.168.50.0/24', '10.0.0.0/8']));
  check('an empty NetID stays its own group key (no prefix fallback)',
    deepEq(prefs.parseRouteCatalogDocument(prefs.serializeRouteCatalogDocument(MGMT,
      { available: true, serial: 1, selection: prefs.routeSelectionDefault(),
        networks: [entry('', ['172.16.0.0/12'], 1, true, false)] }, 'r', 9)).catalog.networks[0],
      entry('', ['172.16.0.0/12'], 1, true, false)));
  check('the empty NetID is toggleable and default-on',
    prefs.routeSelectionIsEnabled(fresh, '') &&
    deepEq(prefs.routeSelectionDisable(fresh, '', ['']).deselected_routes, ['']));
  check('a NetID is preserved verbatim (no trim, no lowercasing)',
    prefs.parseRouteCatalogDocument(prefs.serializeRouteCatalogDocument(MGMT,
      { available: true, serial: 1, selection: prefs.routeSelectionDefault(),
        networks: [entry(' HostInternal ', ['192.168.50.0/24'], 1, true, false)] }, 'r', 9))
      .catalog.networks[0].net_id === ' HostInternal ');

  function catalogDocText(mutate: (doc: any) => void): string {
    const doc: any = {
      v: 1, management_url: MGMT, updated_at_ms: 1000, request_id: 'r', catalog: catalogFixture()
    };
    mutate(doc);
    return JSON.stringify(doc);
  }
  check('a catalog version mismatch is a stable version error',
    reasonOf(() => prefs.parseRouteCatalogDocument(catalogDocText((doc: any) => { doc.v = 2; }))) === 'version');
  const malformedCatalogDocs: string[] = [
    'not-json', 'null',
    catalogDocText((doc: any) => { delete doc.catalog; }),
    catalogDocText((doc: any) => { doc.catalog.serial = 1.5; }),
    catalogDocText((doc: any) => { doc.catalog.serial = -1; }),
    catalogDocText((doc: any) => { doc.catalog.available = 'yes'; }),
    catalogDocText((doc: any) => { doc.catalog.networks[0].candidate_count = 0; }),
    catalogDocText((doc: any) => { doc.catalog.networks[0].candidate_count = 1.5; }),
    catalogDocText((doc: any) => { doc.catalog.networks[0].net_id = 7; }),
    catalogDocText((doc: any) => { doc.catalog.networks[0].networks = 'x'; }),
    catalogDocText((doc: any) => { doc.catalog.networks[0].enabled = 'true'; }),
    catalogDocText((doc: any) => { doc.catalog.networks[0].is_exit = 1; }),
    catalogDocText((doc: any) => { doc.catalog.networks.push(doc.catalog.networks[0]); }),
    catalogDocText((doc: any) => { doc.catalog.networks.push(null); }),
    catalogDocText((doc: any) => { delete doc.updated_at_ms; }),
    catalogDocText((doc: any) => { doc.updated_at_ms = -1; }),
    catalogDocText((doc: any) => { doc.request_id = 7; })
  ];
  check('every malformed catalog document is a stable malformed error',
    malformedCatalogDocs.every((text) =>
      reasonOf(() => prefs.parseRouteCatalogDocument(text)) === 'malformed'));

  const rows = prefs.routeSelectionRows(fresh, catalogFixture());
  check('rows recompute enabled from the selection, not from the catalog snapshot',
    rows[2].is_exit === true && rows[2].enabled === true &&
    prefs.routeSelectionRows(prefs.routeSelectionDisable(fresh, EXIT, KNOWN),
      catalogFixture())[2].enabled === false);
  check('rows follow a per-NetID disable and expose the HA candidate count',
    prefs.routeSelectionRows(off, catalogFixture())[0].enabled === false &&
    prefs.routeSelectionRows(off, catalogFixture())[0].candidate_count === 2 &&
    prefs.routeSelectionRows(off, catalogFixture())[1].enabled === true);
  const catalogObj = catalogFixture();
  const copyRows = prefs.routeSelectionRows(fresh, catalogObj);
  check('rows are fresh copies, not aliases of the catalog',
    copyRows !== catalogObj.networks && copyRows[0] !== catalogObj.networks[0] &&
    copyRows[0].networks !== catalogObj.networks[0].networks);
  check('known NetIDs come back sorted and unique',
    deepEq(prefs.routeSelectionKnownNetIds(catalogFixture()), ['HostInternal', 'corp', 'exit-node']));

  check('a fresh catalog stamp is not stale',
    prefs.routeCatalogIsStale(1000, 10999, 10000) === false);
  check('an old catalog stamp is stale', prefs.routeCatalogIsStale(1000, 11001, 10000) === true);
  check('a zero or future stamp is stale (clock jump safe)',
    prefs.routeCatalogIsStale(0, 2000, 10000) === true &&
    prefs.routeCatalogIsStale(5000, 4000, 10000) === true);

  // ---- file IO -------------------------------------------------------------
  const selectionPath = join(filesDir, store.ROUTE_SELECTION_FILE);
  const selectionTemp = join(filesDir, store.ROUTE_SELECTION_FILE + '.tmp');
  const catalogPath = join(filesDir, store.ROUTE_CATALOG_FILE);
  await writeFile(join(filesDir, 'netbird-device-config.json'), '{"device":"untouched"}');
  await writeFile(join(filesDir, 'netbird-setup-key'), 'untouched');

  check('a missing selection file reads as null (default all-on)',
    await store.loadRouteSelection(filesDir, MGMT) === null);
  const missingCatalog = await store.readRouteCatalog(filesDir, MGMT);
  check('a missing catalog reads as the missing status with no catalog',
    missingCatalog.status === 'missing' && missingCatalog.catalog === null &&
    missingCatalog.updated_at_ms === 0 && missingCatalog.request_id === '');

  await store.saveRouteSelection(filesDir, MGMT, selectionFixture());
  check('the saved selection is the canonical document byte-for-byte',
    (await readFile(selectionPath, 'utf8')) ===
      prefs.serializeRouteSelectionDocument(MGMT, selectionFixture()));
  check('selection round-trips through the file',
    deepEq(await store.loadRouteSelection(filesDir, MGMT), selectionFixture()));
  check('no temp file survives a successful save', !(await exists(selectionTemp)));
  check('the store never writes the device config or the setup key',
    (await readFile(join(filesDir, 'netbird-device-config.json'), 'utf8')) === '{"device":"untouched"}' &&
    (await readFile(join(filesDir, 'netbird-setup-key'), 'utf8')) === 'untouched');

  const savedSelectionBytes = await readFile(selectionPath);
  check('a selection scoped to another management URL reads as absent',
    await store.loadRouteSelection(filesDir, MGMT_OTHER) === null);
  check('the other-server read does not rewrite the file',
    (await readFile(selectionPath)).equals(savedSelectionBytes));

  const brokenSelection = '{"v":1,"management_url":"' + MGMT + '","selection":{"deselect_all":1}}';
  await writeFile(selectionPath, brokenSelection);
  check('a malformed existing selection is a stable error, not a silent default',
    await reasonOfAsync(() => store.loadRouteSelection(filesDir, MGMT)) === 'malformed');
  check('the malformed file is left byte-identical (no accidental re-enable)',
    (await readFile(selectionPath, 'utf8')) === brokenSelection);
  await writeFile(selectionPath,
    '{"v":2,"management_url":"' + MGMT + '","selection":{"selected_routes":[],"deselected_routes":[],"deselect_all":false}}');
  check('a wrong-version selection file is a stable version error',
    await reasonOfAsync(() => store.loadRouteSelection(filesDir, MGMT)) === 'version');

  await writeFile(selectionPath, savedSelectionBytes);
  check('an invalid selection is refused before the file is touched',
    await reasonOfAsync(() => store.saveRouteSelection(filesDir, MGMT,
      { selected_routes: ['a'], deselected_routes: [], deselect_all: 'no' })) === 'malformed' &&
    (await readFile(selectionPath)).equals(savedSelectionBytes));
  check('an unusable filesDir is unavailable, not a fabricated default',
    await reasonOfAsync(() => store.loadRouteSelection('', MGMT)) === 'unavailable');

  state.readError = { code: 13900005 };
  check('a real read failure is unavailable, not a fabricated missing selection',
    await reasonOfAsync(() => store.loadRouteSelection(filesDir, MGMT)) === 'unavailable');
  check('a catalog read failure reports unavailable, not missing',
    (await store.readRouteCatalog(filesDir, MGMT)).status === 'unavailable');
  state.readError = null;

  state.writeError = new Error('WRITE_RAW_SENTINEL');
  check('an interrupted write reports unavailable',
    await reasonOfAsync(() => store.saveRouteSelection(filesDir, MGMT,
      { selected_routes: [], deselected_routes: [HOST], deselect_all: false })) === 'unavailable');
  state.writeError = null;
  check('an interrupted write retains the old file and no temp file',
    (await readFile(selectionPath)).equals(savedSelectionBytes) &&
    deepEq(await store.loadRouteSelection(filesDir, MGMT), selectionFixture()) &&
    !(await exists(selectionTemp)));

  await store.writeRouteCatalog(filesDir, MGMT, catalogFixture(), 'req-1', 1700000000000);
  const okCatalog = await store.readRouteCatalog(filesDir, MGMT);
  check('the written catalog reads back as ok with the exact catalog',
    okCatalog.status === 'ok' && deepEq(okCatalog.catalog, catalogFixture()));
  check('the catalog read reports the cache stamp and request id',
    okCatalog.updated_at_ms === 1700000000000 && okCatalog.request_id === 'req-1' &&
    okCatalog.management_url === MGMT);
  const otherCatalog = await store.readRouteCatalog(filesDir, MGMT_OTHER);
  check('a catalog for another server is a scope mismatch with timestamp but no catalog',
    otherCatalog.status === 'scope-mismatch' && otherCatalog.catalog === null &&
    otherCatalog.updated_at_ms === 1700000000000 && otherCatalog.management_url === MGMT);

  await writeFile(catalogPath, catalogDocText((doc: any) => { doc.v = 2; }));
  check('a wrong-version catalog file reports the version status',
    (await store.readRouteCatalog(filesDir, MGMT)).status === 'version');
  await writeFile(catalogPath, 'not-json');
  check('a malformed catalog file reports the malformed status',
    (await store.readRouteCatalog(filesDir, MGMT)).status === 'malformed');
  await writeFile(catalogPath, Buffer.from([0xff, 0xfe, 0x00, 0x7b]));
  check('a non-UTF-8 catalog file reports malformed, not missing',
    (await store.readRouteCatalog(filesDir, MGMT)).status === 'malformed');

  await store.writeRouteCatalog(filesDir, MGMT, catalogFixture(), 'req-1', 1700000000000);
  const savedCatalogBytes = await readFile(catalogPath);
  state.writeError = new Error('WRITE_RAW_SENTINEL');
  check('an interrupted catalog write reports unavailable',
    await reasonOfAsync(() => store.writeRouteCatalog(filesDir, MGMT,
      { available: false, serial: 1, selection: prefs.routeSelectionDefault(), networks: [] },
      'req-2', 1800000000000)) === 'unavailable');
  state.writeError = null;
  check('an interrupted catalog write retains the old catalog',
    (await readFile(catalogPath)).equals(savedCatalogBytes) &&
    (await store.readRouteCatalog(filesDir, MGMT)).catalog?.serial === 302);
  check('an invalid catalog is refused before the file is touched',
    await reasonOfAsync(() => store.writeRouteCatalog(filesDir, MGMT,
      { available: true, serial: 1, selection: prefs.routeSelectionDefault(),
        networks: [entry('dup', [], 1, true, false), entry('dup', [], 1, true, false)] },
      'req-3', 1900000000000)) === 'malformed' &&
    (await readFile(catalogPath)).equals(savedCatalogBytes));

  // ---- document shape / secret hygiene ------------------------------------
  const writtenSelectionText = await readFile(selectionPath, 'utf8');
  const writtenCatalogText = await readFile(catalogPath, 'utf8');
  check('the selection document has exactly the three wrapper keys',
    deepEq(Object.keys(JSON.parse(writtenSelectionText)), ['v', 'management_url', 'selection']));
  check('the selection object has exactly the three core keys',
    deepEq(Object.keys(JSON.parse(writtenSelectionText).selection),
      ['selected_routes', 'deselected_routes', 'deselect_all']));
  check('the catalog wrapper has exactly the five contract keys',
    deepEq(Object.keys(JSON.parse(writtenCatalogText)),
      ['v', 'management_url', 'updated_at_ms', 'request_id', 'catalog']));
  check('the catalog entry has exactly the five native-contract keys',
    deepEq(Object.keys(JSON.parse(writtenCatalogText).catalog.networks[0]),
      ['net_id', 'networks', 'candidate_count', 'enabled', 'is_exit']));
  check('neither document carries a credential-shaped field',
    !/token|jwt|secret|private_key|password|bearer/i.test(writtenSelectionText + writtenCatalogText));
} finally {
  await rm(temp, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
