/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side live-route IPC checks (no device, no network, no compiler). Same
// principle as run-route-preferences.mts: the REAL production modules are
// copied byte-for-byte into an OS-temp fixture (.ts) at run time — the code
// under test is the shipped code, never a hand-held mirror:
//   NetBirdRouteRequest.ets       (pure apply-request / apply-result documents)
//   NetBirdRouteRequestStore.ets  (atomic file IO; @ohos.file.fs / @ohos.util
//                                  are functional stubs over the real node fs)
//   NetBirdRoutePreferences.ets   (only for the shared error vocabulary and
//                                  bounds the request module imports)
//
// Coverage:
//   E  exported contract: document types, version/state/mode tokens, parse and
//      serialize functions, the store API and the two file names
//   P  pure documents: exact canonical bytes, roundtrip, key sets (no selection
//      copy, no session field), version errors, malformed shapes, mode '' ,
//      route-list canonicalization, the stored-no-map-is-not-applied predicate
//   F  file IO: missing file -> null, roundtrip, write-before-touch validation,
//      malformed / wrong-version / non-UTF-8 / oversized files are stable
//      errors and the file is left byte-identical, IO failure is unavailable
//      (not "missing"), interrupted write retains the old document and no temp
//      file, an unusable filesDir is unavailable, every read is a real read
//      (equal-size consecutive updates still expose the newer id), the request
//      is retained (never unlinked) and the pre-existing preference / device
//      files are byte-untouched, no credential-shaped payload
//
// Run: node client/entry/selfcheck/run-route-request.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFile, writeFile, mkdtemp, rm, mkdir, open, rename, unlink, access, readdir, stat, utimes } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./route-request-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const routesDir = join(here, '..', 'src', 'main', 'ets', 'routes');
const temp = await mkdtemp(join(tmpdir(), 'nb-route-request-'));
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

const state: {
  nodeFs: { open: typeof open; rename: typeof rename; unlink: typeof unlink };
  readError: { code: number } | null;
  writeError: Error | null;
} = {
  nodeFs: { open, rename, unlink },
  readError: null,
  writeError: null
};
(globalThis as unknown as { __NB_ROUTE_REQUEST_TEST: typeof state }).__NB_ROUTE_REQUEST_TEST = state;

// The request the UI writes: an opaque id + the scope that changed. NO selection
// copy and NO session identity, exactly as agreed.
function requestFixture(id = 'ui-1712345678901-7', managementUrl = MGMT, createdAtMs = 1712345678901) {
  return { id, management_url: managementUrl, created_at_ms: createdAtMs };
}

function resultFixture(): {
  id: string; state: string; mode: string; error: string; routes: string[]; at_ms: number;
} {
  return {
    id: 'ui-1712345678901-7',
    state: 'applied',
    mode: 'recreated',
    error: '',
    routes: ['HostInternal', 'corp'],
    at_ms: 1712345679999
  };
}

// A raw document as some writer could have left it: the wrapper version is
// present (as a real file always carries it) and only the named field is bent.
function requestText(mutate: (doc: any) => void): string {
  const doc: any = { v: 1, ...JSON.parse(JSON.stringify(requestFixture())) };
  mutate(doc);
  return JSON.stringify(doc);
}

function resultText(mutate: (doc: any) => void): string {
  const doc: any = { v: 1, ...JSON.parse(JSON.stringify(resultFixture())) };
  mutate(doc);
  return JSON.stringify(doc);
}

try {
  // ---- fixtures are byte-identical -----------------------------------------
  const names = ['NetBirdRouteRequest.ets', 'NetBirdRouteRequestStore.ets', 'NetBirdRoutePreferences.ets'];
  for (const name of names) {
    const source = join(routesDir, name);
    await writeFile(join(fixture, name.replace('.ets', '.ts')), await readFile(source));
    check(`${name} fixture is byte-identical`,
      (await readFile(source)).equals(await readFile(join(fixture, name.replace('.ets', '.ts')))));
  }

  const request = await import(pathToFileURL(join(fixture, 'NetBirdRouteRequest.ts')).href) as Record<string, any>;
  const store = await import(pathToFileURL(join(fixture, 'NetBirdRouteRequestStore.ts')).href) as Record<string, any>;

  // ---- exported contract ---------------------------------------------------
  const pureFns = ['serializeRouteApplyRequestDocument', 'parseRouteApplyRequestDocument',
    'serializeRouteApplyResultDocument', 'parseRouteApplyResultDocument',
    'routeApplyResultIsApplied'];
  const tokens = ['ROUTE_APPLY_STATE_APPLYING', 'ROUTE_APPLY_STATE_APPLIED', 'ROUTE_APPLY_STATE_FAILED',
    'ROUTE_APPLY_MODE_NONE', 'ROUTE_APPLY_MODE_NO_OS_CHANGE', 'ROUTE_APPLY_MODE_RECREATED',
    'ROUTE_APPLY_MODE_STORED_NO_MAP'];
  check('the pure module exports every parse/format function, token and version',
    pureFns.every((name) => typeof request[name] === 'function') &&
    tokens.every((name) => typeof request[name] === 'string') &&
    typeof request.ROUTE_APPLY_REQUEST_VERSION === 'number' &&
    typeof request.ROUTE_APPLY_RESULT_VERSION === 'number');
  check('the contract token values are the agreed ones',
    request.ROUTE_APPLY_REQUEST_VERSION === 1 && request.ROUTE_APPLY_RESULT_VERSION === 1 &&
    request.ROUTE_APPLY_STATE_APPLYING === 'applying' &&
    request.ROUTE_APPLY_STATE_APPLIED === 'applied' &&
    request.ROUTE_APPLY_STATE_FAILED === 'failed' &&
    request.ROUTE_APPLY_MODE_NONE === '' &&
    request.ROUTE_APPLY_MODE_NO_OS_CHANGE === 'no-os-change' &&
    request.ROUTE_APPLY_MODE_RECREATED === 'recreated' &&
    request.ROUTE_APPLY_MODE_STORED_NO_MAP === 'stored-no-map');
  const storeFns = ['writeRouteApplyRequest', 'readRouteApplyRequest',
    'writeRouteApplyResult', 'readRouteApplyResult'];
  check('the store exports the four async API functions and the two file names',
    storeFns.every((name) => typeof store[name] === 'function') &&
    store.ROUTE_APPLY_REQUEST_FILE === 'netbird-route-apply-request.json' &&
    store.ROUTE_APPLY_RESULT_FILE === 'netbird-route-apply-result.json' &&
    store.MAX_ROUTE_APPLY_DOCUMENT_BYTES === 16 * 1024);

  // ---- pure request document ----------------------------------------------
  check('the request document is exactly the four agreed fields',
    request.serializeRouteApplyRequestDocument(requestFixture()) ===
      '{"v":1,"id":"ui-1712345678901-7","management_url":"' + MGMT + '","created_at_ms":1712345678901}');
  check('the request round-trips through its own bytes',
    deepEq(request.parseRouteApplyRequestDocument(
      request.serializeRouteApplyRequestDocument(requestFixture())), requestFixture()));
  check('the request wrapper has exactly the four contract keys',
    deepEq(Object.keys(JSON.parse(request.serializeRouteApplyRequestDocument(requestFixture()))),
      ['v', 'id', 'management_url', 'created_at_ms']));
  check('the request copies no selection and binds no session',
    !/selection|deselect|selected_routes|session|request_id|token/i.test(
      request.serializeRouteApplyRequestDocument(requestFixture())));

  const malformedRequests = [
    'not-json',
    'null',
    '[]',
    '"x"',
    requestText((d) => { delete d.id; }),
    requestText((d) => { d.id = ''; }),
    requestText((d) => { d.id = 'x'.repeat(257); }),
    requestText((d) => { d.id = 7; }),
    requestText((d) => { delete d.management_url; }),
    requestText((d) => { d.management_url = ''; }),
    requestText((d) => { d.management_url = 'https://a b'; }),
    requestText((d) => { d.management_url = 'x'.repeat(2049); }),
    requestText((d) => { d.management_url = 7; }),
    requestText((d) => { delete d.created_at_ms; }),
    requestText((d) => { d.created_at_ms = -1; }),
    requestText((d) => { d.created_at_ms = 1.5; }),
    requestText((d) => { d.created_at_ms = '1712345678901'; })
  ];
  check('every malformed request document is a stable malformed error',
    malformedRequests.every((text) =>
      reasonOf(() => request.parseRouteApplyRequestDocument(text)) === 'malformed'));
  check('a wrong-version request document is a stable version error',
    reasonOf(() => request.parseRouteApplyRequestDocument(requestText((d) => { d.v = 2; }))) === 'version' &&
    reasonOf(() => request.parseRouteApplyRequestDocument(requestText((d) => { d.v = '1'; }))) === 'version' &&
    reasonOf(() => request.parseRouteApplyRequestDocument(requestText((d) => { delete d.v; }))) === 'version');
  check('an invalid request is refused before serialization produces bytes',
    reasonOf(() => request.serializeRouteApplyRequestDocument(
      { id: '', management_url: MGMT, created_at_ms: 1 })) === 'malformed' &&
    reasonOf(() => request.serializeRouteApplyRequestDocument(
      { id: 'ui-1', management_url: MGMT, created_at_ms: 1.5 })) === 'malformed' &&
    reasonOf(() => request.serializeRouteApplyRequestDocument(null)) === 'malformed');

  // ---- pure result document ------------------------------------------------
  const result = resultFixture();
  check('the result document is exactly the seven agreed fields',
    request.serializeRouteApplyResultDocument(result) ===
      '{"v":1,"id":"ui-1712345678901-7","state":"applied","mode":"recreated","error":"",' +
      '"routes":["HostInternal","corp"],"at_ms":1712345679999}');
  check('the result round-trips through its own bytes',
    deepEq(request.parseRouteApplyResultDocument(
      request.serializeRouteApplyResultDocument(result)), result));
  check('the result wrapper has exactly the seven contract keys',
    deepEq(Object.keys(JSON.parse(request.serializeRouteApplyResultDocument(result))),
      ['v', 'id', 'state', 'mode', 'error', 'routes', 'at_ms']));
  check('the result route list is canonicalized (sorted, deduplicated)',
    deepEq(JSON.parse(request.serializeRouteApplyResultDocument(
      { ...result, routes: ['corp', 'HostInternal', 'corp'] })).routes,
      ['HostInternal', 'corp']) &&
    deepEq(request.parseRouteApplyResultDocument(
      resultText((d) => { d.routes = ['corp', 'HostInternal']; })).routes,
      ['HostInternal', 'corp']));
  check('every legal state and mode token parses, including the empty mode',
    ['applying', 'applied', 'failed'].every((value) =>
      request.parseRouteApplyResultDocument(
        resultText((d) => { d.state = value; d.mode = ''; })).state === value) &&
    [['no-os-change', 'applied'], ['recreated', 'applied'], ['stored-no-map', 'failed']].every((pair) =>
      request.parseRouteApplyResultDocument(
        resultText((d) => { d.mode = pair[0]; d.state = pair[1]; })).mode === pair[0]));

  const malformedResults = [
    'not-json',
    'null',
    '[]',
    '3',
    resultText((d) => { delete d.id; }),
    resultText((d) => { d.id = ''; }),
    resultText((d) => { delete d.state; }),
    resultText((d) => { d.state = 'done'; }),
    resultText((d) => { d.state = 1; }),
    resultText((d) => { delete d.mode; }),
    resultText((d) => { d.mode = 'recreate'; }),
    resultText((d) => { d.mode = null; }),
    resultText((d) => { delete d.error; }),
    resultText((d) => { d.error = 7; }),
    resultText((d) => { delete d.routes; }),
    resultText((d) => { d.routes = 'all'; }),
    resultText((d) => { d.routes = ['a', 7]; }),
    resultText((d) => { d.routes = ['']; }),
    resultText((d) => { d.routes = ['x'.repeat(513)]; }),
    resultText((d) => { delete d.at_ms; }),
    resultText((d) => { d.at_ms = -1; }),
    resultText((d) => { d.at_ms = 1.5; }),
    resultText((d) => { d.at_ms = '1712345679999'; })
  ];
  check('every malformed result document is a stable malformed error',
    malformedResults.every((text) =>
      reasonOf(() => request.parseRouteApplyResultDocument(text)) === 'malformed'));
  check('a wrong-version result document is a stable version error',
    reasonOf(() => request.parseRouteApplyResultDocument(resultText((d) => { d.v = 2; }))) === 'version' &&
    reasonOf(() => request.parseRouteApplyResultDocument(resultText((d) => { d.v = '1'; }))) === 'version');
  check('an invalid result is refused before serialization produces bytes',
    reasonOf(() => request.serializeRouteApplyResultDocument(
      { ...result, state: 'done' })) === 'malformed' &&
    reasonOf(() => request.serializeRouteApplyResultDocument(
      { ...result, mode: 'recreate' })) === 'malformed' &&
    reasonOf(() => request.serializeRouteApplyResultDocument(
      { ...result, routes: [''] })) === 'malformed' &&
    reasonOf(() => request.serializeRouteApplyResultDocument(undefined)) === 'malformed');

  // ---- stored-no-map is NOT applied ---------------------------------------
  const applied = (s: string, m: string) =>
    request.routeApplyResultIsApplied({ ...result, state: s, mode: m });
  check('only an applied state with no stored-no-map counts as applied',
    applied('applied', 'recreated') === true &&
    applied('applied', 'no-os-change') === true &&
    applied('applied', 'stored-no-map') === false &&
    applied('applying', '') === false &&
    applied('failed', 'stored-no-map') === false &&
    applied('failed', '') === false);

  // ---- file IO -------------------------------------------------------------
  const requestPath = join(filesDir, store.ROUTE_APPLY_REQUEST_FILE);
  const requestTemp = requestPath + '.tmp';
  const resultPath = join(filesDir, store.ROUTE_APPLY_RESULT_FILE);
  const resultTemp = resultPath + '.tmp';
  // Pre-existing documents this module must never touch (the preference
  // document is the one the UI saves BEFORE the request).
  const prefsText = '{"v":1,"management_url":"' + MGMT +
    '","selection":{"selected_routes":["HostInternal"],"deselected_routes":["corp"],"deselect_all":false}}';
  await writeFile(join(filesDir, 'netbird-route-selection.json'), prefsText);
  await writeFile(join(filesDir, 'netbird-device-config.json'), '{"device":"untouched"}');
  await writeFile(join(filesDir, 'netbird-setup-key'), 'untouched');

  check('a missing request reads as null', await store.readRouteApplyRequest(filesDir) === null);
  check('a missing result reads as null', await store.readRouteApplyResult(filesDir) === null);

  await store.writeRouteApplyRequest(filesDir, requestFixture());
  check('the written request is the canonical document byte-for-byte',
    (await readFile(requestPath, 'utf8')) ===
      request.serializeRouteApplyRequestDocument(requestFixture()));
  check('the request round-trips through the file',
    deepEq(await store.readRouteApplyRequest(filesDir), requestFixture()));
  check('no temp file survives a successful request write', !(await exists(requestTemp)));
  check('reading the request retains it (the mailbox is never unlinked)',
    await exists(requestPath) &&
    (await readFile(requestPath, 'utf8')) ===
      request.serializeRouteApplyRequestDocument(requestFixture()));

  await store.writeRouteApplyResult(filesDir, resultFixture());
  check('the written result is the canonical document byte-for-byte',
    (await readFile(resultPath, 'utf8')) === request.serializeRouteApplyResultDocument(resultFixture()));
  check('the result round-trips through the file',
    deepEq(await store.readRouteApplyResult(filesDir), resultFixture()));
  check('no temp file survives a successful result write', !(await exists(resultTemp)));

  // The request is a scope NOTIFICATION: the store does not filter it, because
  // the reader loads the selection with the CURRENT management URL itself.
  const otherRequest = requestFixture('ui-other-1', MGMT_OTHER, 1712345600000);
  await store.writeRouteApplyRequest(filesDir, otherRequest);
  check('a request for another management URL is returned verbatim (no scope filter here)',
    deepEq(await store.readRouteApplyRequest(filesDir), otherRequest));

  // Same-size consecutive atomic writes: a size/mtime "unchanged" shortcut would
  // miss the newer id, so every read must be a real read.
  let sameSize = true;
  let latestVisible = true;
  for (let i = 0; i < 10; i++) {
    const older = requestFixture(`ui-loop-${10 + i}a`, MGMT, 1712345600000);
    const newer = requestFixture(`ui-loop-${10 + i}b`, MGMT, 1712345600001);
    await store.writeRouteApplyRequest(filesDir, older);
    const olderBytes = await readFile(requestPath);
    await store.writeRouteApplyRequest(filesDir, newer);
    const newerBytes = await readFile(requestPath);
    sameSize = sameSize && olderBytes.length === newerBytes.length;
    const read = await store.readRouteApplyRequest(filesDir);
    latestVisible = latestVisible && read !== null && read.id === newer.id;
  }
  check('consecutive same-size atomic writes produce equal byte counts', sameSize);
  check('the newest same-size request is always visible to a real read', latestVisible);

  // Decisive negative control for the size/mtime shortcut: overwrite the
  // request with a DIFFERENT document of the SAME byte count and hand it back
  // the SAME millisecond mtime the reader last observed. A size/mtime "nothing
  // changed" guard cannot tell the two apart; only a real read sees the newer id.
  const stableA = requestFixture('ui-stable-1', MGMT, 1712345600000);
  const stableB = requestFixture('ui-stable-2', MGMT, 1712345600000);
  await store.writeRouteApplyRequest(filesDir, stableA);
  const stampA = await stat(requestPath);
  const observedMtimeMs = Math.round(stampA.mtimeMs);
  await writeFile(requestPath, request.serializeRouteApplyRequestDocument(stableB));
  await utimes(requestPath, stampA.atime, stampA.mtime);
  const stampB = await stat(requestPath);
  const readStable = await store.readRouteApplyRequest(filesDir);
  check('a same-size, same-millisecond-mtime overwrite still exposes the newer id (no metadata shortcut)',
    stampA.size === stampB.size && Math.round(stampB.mtimeMs) === observedMtimeMs &&
    readStable !== null && readStable.id === stableB.id);

  const savedRequestBytes = await readFile(requestPath);
  const savedResultBytes = await readFile(resultPath);
  const brokenRequest = '{"v":1,"id":';
  await writeFile(requestPath, brokenRequest);
  check('a malformed existing request is a stable malformed error',
    await reasonOfAsync(() => store.readRouteApplyRequest(filesDir)) === 'malformed');
  check('the malformed request file is left byte-identical',
    (await readFile(requestPath, 'utf8')) === brokenRequest);
  await writeFile(requestPath, requestText((d) => { d.v = 2; }));
  check('a wrong-version request file is a stable version error',
    await reasonOfAsync(() => store.readRouteApplyRequest(filesDir)) === 'version');
  await writeFile(requestPath, Buffer.from([0xff, 0xfe, 0x00, 0x7b]));
  check('a non-UTF-8 request file is malformed, not missing',
    await reasonOfAsync(() => store.readRouteApplyRequest(filesDir)) === 'malformed');
  await writeFile(requestPath,
    request.serializeRouteApplyRequestDocument(requestFixture()) + ' '.repeat(20000));
  check('an oversized request file is malformed (the 16 KiB bound is enforced on read)',
    await reasonOfAsync(() => store.readRouteApplyRequest(filesDir)) === 'malformed');

  await writeFile(requestPath, savedRequestBytes);
  await writeFile(resultPath, savedResultBytes);
  check('an invalid request is refused before the file is touched',
    await reasonOfAsync(() => store.writeRouteApplyRequest(filesDir,
      { id: '', management_url: MGMT, created_at_ms: 1 })) === 'malformed' &&
    (await readFile(requestPath)).equals(savedRequestBytes));
  check('an invalid result is refused before the file is touched',
    await reasonOfAsync(() => store.writeRouteApplyResult(filesDir,
      { ...resultFixture(), state: 'done' })) === 'malformed' &&
    (await readFile(resultPath)).equals(savedResultBytes));
  check('an unusable filesDir is unavailable, not a fabricated null',
    await reasonOfAsync(() => store.readRouteApplyRequest('')) === 'unavailable' &&
    await reasonOfAsync(() => store.writeRouteApplyResult('', resultFixture())) === 'unavailable');

  state.readError = { code: 13900005 };
  check('a real read failure is unavailable for both documents, not "missing"',
    await reasonOfAsync(() => store.readRouteApplyRequest(filesDir)) === 'unavailable' &&
    await reasonOfAsync(() => store.readRouteApplyResult(filesDir)) === 'unavailable');
  state.readError = null;

  state.writeError = new Error('WRITE_RAW_SENTINEL');
  check('an interrupted request write reports unavailable',
    await reasonOfAsync(() => store.writeRouteApplyRequest(filesDir,
      requestFixture('ui-new-1', MGMT, 1712345609999))) === 'unavailable');
  check('an interrupted result write reports unavailable',
    await reasonOfAsync(() => store.writeRouteApplyResult(filesDir,
      { ...resultFixture(), id: 'ui-new-1' })) === 'unavailable');
  state.writeError = null;
  check('an interrupted write retains the old documents and no temp file',
    (await readFile(requestPath)).equals(savedRequestBytes) &&
    (await readFile(resultPath)).equals(savedResultBytes) &&
    !(await exists(requestTemp)) && !(await exists(resultTemp)));

  // ---- own files only / untouched neighbours ------------------------------
  check('the pre-existing preference document is byte-untouched',
    (await readFile(join(filesDir, 'netbird-route-selection.json'), 'utf8')) === prefsText);
  check('the store never writes the device config or the setup key',
    (await readFile(join(filesDir, 'netbird-device-config.json'), 'utf8')) === '{"device":"untouched"}' &&
    (await readFile(join(filesDir, 'netbird-setup-key'), 'utf8')) === 'untouched');
  const entries = (await readdir(filesDir)).sort();
  check('the adapter created exactly its own two files and left no temp sibling',
    deepEq(entries, ['netbird-device-config.json', 'netbird-route-apply-request.json',
      'netbird-route-apply-result.json', 'netbird-route-selection.json', 'netbird-setup-key']
      .sort()));
  check('neither document carries a credential-shaped field',
    !/token|jwt|secret|private_key|password|bearer|setup_key/i.test(
      (await readFile(requestPath, 'utf8')) + (await readFile(resultPath, 'utf8'))));
} finally {
  await rm(temp, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
