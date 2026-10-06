/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side encrypted account-store checks. Production .ets files are copied
// byte-for-byte; platform APIs are faked, while AES-GCM is real Node crypto.
// Run: node --import ./client/entry/selfcheck/account-store-hooks.mjs \
//   client/entry/selfcheck/run-account-store.mts

import { readFile, writeFile, mkdtemp, rm, mkdir, open, rename, unlink, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./account-store-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const loginDir = join(here, '..', 'src', 'main', 'ets', 'login');
const temp = await mkdtemp(join(tmpdir(), 'nb-account-store-'));
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
function alias(query: Map<number, boolean | number | Uint8Array>): string {
  const bytes = query.get(0x03000002);
  return bytes instanceof Uint8Array ? String.fromCharCode(...bytes) : '';
}
const state: {
  key: Uint8Array | null;
  aliasName: string;
  nodeFs: { open: typeof open; rename: typeof rename; unlink: typeof unlink };
  alias: typeof alias;
  readError: { code: number } | null;
  writeError: Error | null;
} = {
  key: null,
  aliasName: '',
  nodeFs: { open, rename, unlink },
  alias,
  readError: null,
  writeError: null
};
(globalThis as unknown as { __NB_STORE_TEST: typeof state }).__NB_STORE_TEST = state;

try {
  const names = ['NetBirdAccountStore.ets', 'NetBirdAccountKeyStore.ets', 'NetBirdAccountStoreCore.ets'];
  for (const name of names) {
    const source = join(loginDir, name);
    await writeFile(join(fixture, name.replace('.ets', '.ts')), await readFile(source));
    check(`${name} fixture is byte-identical`,
      (await readFile(source)).equals(await readFile(join(fixture, name.replace('.ets', '.ts')))));
  }
  const module = await import(pathToFileURL(join(fixture, 'NetBirdAccountStore.ts')).href) as Record<string, any>;
  const core = await import(pathToFileURL(join(fixture, 'NetBirdAccountStoreCore.ts')).href) as Record<string, any>;
  const creds = {
    managementUrl: 'https://management.example.com',
    clientId: 'netbird-client',
    tokenEndpoint: 'https://idp.example.com/oauth/token',
    useIdToken: false,
    jwt: 'A'.repeat(8 * 1024),
    refreshToken: `refresh-${'R'.repeat(1700)}`,
    expiresAtMs: 1234567890
  };
  check('exports exact async store API',
    typeof module.saveAccountCredentials === 'function' &&
    typeof module.loadAccountCredentials === 'function' &&
    typeof module.clearAccountCredentials === 'function');

  check('missing account returns null', await module.loadAccountCredentials(filesDir) === null);
  await module.saveAccountCredentials(filesDir, creds);
  const accountPath = join(filesDir, 'netbird-account.enc');
  const originalBytes = await readFile(accountPath);
  const envelope = JSON.parse(originalBytes.toString('utf8'));
  check('envelope encodes expected IV, ciphertext, and tag byte lengths',
    Buffer.from(envelope.iv, 'base64').length === 12 && Buffer.from(envelope.ciphertext, 'base64').length > 0 &&
    Buffer.from(envelope.tag, 'base64').length === 16);
  check('8 KiB JWT saves and round-trips; store token bound is 16 KiB',
    core.MAX_TOKEN_CHARS === 16 * 1024 &&
    JSON.stringify(await module.loadAccountCredentials(filesDir)) === JSON.stringify(creds));
  let oversizedReason = '';
  try {
    await module.saveAccountCredentials(filesDir, { ...creds, jwt: 'J'.repeat(core.MAX_TOKEN_CHARS + 1) });
  } catch (error) { oversizedReason = (error as { reason?: string }).reason ?? ''; }
  check('JWT above store token bound is rejected without replacing saved credentials',
    oversizedReason === 'malformed' && (await readFile(accountPath)).equals(originalBytes));
  check('file contains no JWT, refresh token, or management URL plaintext',
    !originalBytes.includes(Buffer.from(creds.jwt)) && !originalBytes.includes(Buffer.from(creds.refreshToken)) &&
    !originalBytes.includes(Buffer.from(creds.managementUrl)));
  check('AssetStoreKit contains only a 32-byte key, not credentials',
    state.key?.length === 32 && !Buffer.from(state.key ?? []).includes(Buffer.from(creds.jwt)));

  let mismatch = '';
  try { await module.loadAccountCredentials(filesDir, 'https://other.example.com'); }
  catch (error) { mismatch = (error as { reason?: string }).reason ?? ''; }
  check('expected management URL mismatch has stable mismatch reason', mismatch === 'mismatch');

  const badReadDir = join(temp, 'bad-read');
  await mkdir(badReadDir);
  state.readError = { code: 13900005 };
  let readFailure = '';
  try { await module.loadAccountCredentials(badReadDir); }
  catch (error) { readFailure = (error as { reason?: string }).reason ?? ''; }
  state.readError = null;
  check('read failure is unavailable, not a fabricated missing account', readFailure === 'unavailable');

  const tamperCases: Array<{ label: string; change: (doc: any) => void }> = [
    { label: 'nonce', change: (doc: any) => { const b = Buffer.from(doc.iv, 'base64'); b[0] ^= 1; doc.iv = b.toString('base64'); } },
    { label: 'tag', change: (doc: any) => { const b = Buffer.from(doc.tag, 'base64'); b[0] ^= 1; doc.tag = b.toString('base64'); } },
    { label: 'invalid tag length', change: (doc: any) => { doc.tag = Buffer.from([1]).toString('base64'); } },
    { label: 'ciphertext', change: (doc: any) => { const b = Buffer.from(doc.ciphertext, 'base64'); b[0] ^= 1; doc.ciphertext = b.toString('base64'); } }
  ];
  for (const test of tamperCases) {
    const changed = JSON.parse(originalBytes.toString('utf8'));
    test.change(changed);
    await writeFile(accountPath, JSON.stringify(changed));
    let reason = '';
    try { await module.loadAccountCredentials(filesDir); }
    catch (error) { reason = (error as { reason?: string }).reason ?? ''; }
    check(`${test.label} tampering fails closed as malformed`, reason === 'malformed');
  }
  await writeFile(accountPath, originalBytes);

  state.writeError = new Error('WRITE_RAW_SENTINEL');
  let writeFailure = '';
  try { await module.saveAccountCredentials(filesDir, { ...creds, jwt: 'replacement-token' }); }
  catch (error) { writeFailure = (error as Error).message; }
  state.writeError = null;
  check('interrupted temp write reports stable error and leaves old file intact',
    writeFailure === 'unavailable' && (await readFile(accountPath)).equals(originalBytes));
  check('old credentials remain loadable after interrupted write',
    (await module.loadAccountCredentials(filesDir))?.jwt === creds.jwt);

  state.keyError = { code: 202 };
  let keyFailure = '';
  try { await module.loadAccountCredentials(filesDir); }
  catch (error) { keyFailure = (error as { reason?: string }).reason ?? ''; }
  state.keyError = null;
  check('KeyStore unsupported error class preserves stable unsupported reason', keyFailure === 'unsupported');

  await writeFile(join(filesDir, 'netbird-setup-key'), 'untouched');
  await module.clearAccountCredentials(filesDir);
  let accountExists = true;
  try { await access(accountPath); } catch (error) { accountExists = false; }
  check('clear removes only encrypted account file and retains the account key',
    !accountExists && (await readFile(join(filesDir, 'netbird-setup-key'), 'utf8')) === 'untouched' &&
    state.key?.length === 32);
} finally {
  await rm(temp, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
