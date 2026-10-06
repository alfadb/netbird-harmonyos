/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host checks for the exact platform adapter module. The .ets source is copied
// byte-for-byte to a temporary .ts fixture; loader hooks replace only the two
// HarmonyOS modules with fakes. No device assets or real random API are used.
// Run: node --import ./client/entry/selfcheck/account-keystore-hooks.mjs \
//   client/entry/selfcheck/run-account-keystore.mts

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./account-keystore-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', 'src', 'main', 'ets', 'login', 'NetBirdAccountKeyStore.ets');
const temp = mkdtempSync(join(tmpdir(), 'nb-account-keystore-'));
const fixture = join(temp, 'NetBirdAccountKeyStore.ts');
const sourceBytes = readFileSync(source);
writeFileSync(fixture, sourceBytes);
let passed = 0;
let failed = 0;
function check(name: string, ok: boolean): void {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}`);
  if (ok) { passed++; } else { failed++; }
}
function key(seed: number): Uint8Array {
  return new Uint8Array(Array.from({ length: 32 }, (_: number, i: number) => (seed + i) & 0xff));
}
function aliasOf(map: Map<number, boolean | number | Uint8Array>): string | null {
  const bytes = map.get(0x03000002);
  return bytes instanceof Uint8Array ? String.fromCharCode(...bytes) : null;
}

const records = new Map<string, Uint8Array>();
let addError: { code: number; message: string } | null = null;
let queryError: { code: number; message: string } | null = null;
let removeError: { code: number; message: string } | null = null;
let randomBytes = key(17);
let queryCount = 0;
let lastRemove: Map<number, boolean | number | Uint8Array> | null = null;
const assetFake = {
  Tag: { SECRET: 0x03000001, ALIAS: 0x03000002, ACCESSIBILITY: 0x02000003,
    AUTH_TYPE: 0x02000005, SYNC_TYPE: 0x02000010, RETURN_TYPE: 0x02000040,
    RETURN_LIMIT: 0x02000041, CONFLICT_RESOLUTION: 0x02000044 },
  ReturnType: { ALL: 0 },
  Accessibility: { DEVICE_FIRST_UNLOCKED: 1 },
  AuthType: { NONE: 0 },
  SyncType: { NEVER: 0 },
  ConflictResolution: { THROW_ERROR: 1 },
  async query(query: Map<number, boolean | number | Uint8Array>) {
    queryCount++;
    if (queryError !== null) { throw queryError; }
    const alias = aliasOf(query);
    const found = alias === null ? undefined : records.get(alias);
    if (found === undefined) { throw { code: 24000002, message: 'missing' }; }
    return [new Map([[0x03000001, new Uint8Array(found)]])];
  },
  async add(attributes: Map<number, boolean | number | Uint8Array>) {
    const alias = aliasOf(attributes);
    if (alias === null) { throw { code: 401, message: 'bad alias' }; }
    if (addError !== null) { throw addError; }
    if (records.has(alias)) { throw { code: 24000003, message: 'duplicate' }; }
    if (attributes.get(0x02000003) !== 1 || attributes.get(0x02000005) !== 0 ||
      attributes.get(0x02000010) !== 0 || attributes.get(0x02000044) !== 1) {
      throw { code: 401, message: 'bad attributes' };
    }
    const secret = attributes.get(0x03000001);
    if (!(secret instanceof Uint8Array)) { throw { code: 401, message: 'bad secret' }; }
    records.set(alias, new Uint8Array(secret));
  },
  async remove(query: Map<number, boolean | number | Uint8Array>) {
    lastRemove = query;
    if (removeError !== null) { throw removeError; }
    const alias = aliasOf(query);
    if (alias === null || !records.delete(alias)) { throw { code: 24000002, message: 'missing' }; }
  }
};
(globalThis as unknown as { __NB_KEY_ASSET: object }).__NB_KEY_ASSET = assetFake;
(globalThis as unknown as { __NB_KEY_CRYPTO: object }).__NB_KEY_CRYPTO = {
  createRandom: () => ({ generateRandom: async (length: number) => ({ data: randomBytes.slice(0, length) }) })
};

try {
  check('fixture is byte-identical to production .ets', readFileSync(fixture).equals(sourceBytes));
  const m = await import(pathToFileURL(fixture).href) as Record<string, any>;
  const ownAlias = 'netbird-account-key-v1';
  const otherAlias = 'other-key';
  check('exports only the requested operations and fixed alias',
    m.ACCOUNT_KEY_ALIAS === ownAlias && typeof m.loadOrCreateAccountKey === 'function' &&
    typeof m.loadAccountKey === 'function' && typeof m.clearAccountKey === 'function');

  check('missing alias returns null', await m.loadAccountKey() === null);
  const created = await m.loadOrCreateAccountKey() as Uint8Array;
  check('missing alias creates and returns a 32-byte random key',
    created.length === 32 && created.every((value: number, i: number) => value === randomBytes[i]) &&
    records.get(ownAlias)?.length === 32);
  check('asset query requests plaintext result and a single record',
    queryCount >= 2);
  const loaded = await m.loadAccountKey() as Uint8Array;
  check('existing key loads byte-for-byte without replacement',
    loaded.length === 32 && loaded.every((value: number, i: number) => value === created[i]));

  records.delete(ownAlias);
  addError = null;
  randomBytes = key(80);
  const concurrentWinner = key(140);
  assetFake.add = async (attributes: Map<number, boolean | number | Uint8Array>) => {
    const alias = aliasOf(attributes);
    if (alias !== null) { records.set(alias, concurrentWinner); }
    throw { code: 24000003, message: 'racing duplicate' };
  };
  const raced = await m.loadOrCreateAccountKey() as Uint8Array;
  check('duplicate add race reloads and returns the winning stored key',
    raced.every((value: number, i: number) => value === concurrentWinner[i]));

  queryError = { code: 24009999, message: 'RAW_PLATFORM_SENTINEL' };
  let failedLoad: Error | null = null;
  try { await m.loadAccountKey(); } catch (error) { failedLoad = error as Error; }
  check('non-missing query failure throws stable error, never null or raw SDK text',
    failedLoad !== null && (failedLoad as any).reason === 'unavailable' &&
    failedLoad.message === 'unavailable' && failedLoad.message.indexOf('RAW_PLATFORM_SENTINEL') < 0);
  queryError = null;

  records.set(ownAlias, new Uint8Array(31));
  let malformed: Error | null = null;
  try { await m.loadAccountKey(); } catch (error) { malformed = error as Error; }
  check('stored key with malformed length fails closed',
    malformed !== null && (malformed as any).reason === 'malformed');

  records.set(ownAlias, key(1));
  records.set(otherAlias, key(2));
  await m.clearAccountKey();
  check('clear removes only its own alias and sends an alias-only query',
    !records.has(ownAlias) && records.has(otherAlias) && lastRemove !== null &&
    lastRemove.size === 1 && aliasOf(lastRemove) === ownAlias);

  records.delete(ownAlias);
  const concurrentPrior = key(210);
  addError = { code: 24000001, message: 'ADD_RAW_SENTINEL' };
  assetFake.add = async (attributes: Map<number, boolean | number | Uint8Array>) => {
    const alias = aliasOf(attributes);
    if (alias !== null) { records.set(alias, concurrentPrior); }
    throw addError;
  };
  let failedCreate: Error | null = null;
  try { await m.loadOrCreateAccountKey(); } catch (error) { failedCreate = error as Error; }
  check('add failure is explicit and does not masquerade as a missing key',
    failedCreate !== null && (failedCreate as any).reason === 'unavailable' &&
    failedCreate.message === 'unavailable');
  check('a concurrent existing asset survives every non-duplicate add failure',
    records.get(ownAlias)?.every((value: number, i: number) => value === concurrentPrior[i]) === true);
} finally {
  rmSync(temp, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
