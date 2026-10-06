/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side PKCE crypto-adapter check. The shipped .ets files are imported
// byte-identically; the loader replaces only the HarmonyOS crypto/util APIs.
// Run: node client/entry/selfcheck/run-account-crypto.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { register } from 'node:module';

register(new URL('./account-crypto-hooks.mjs', import.meta.url).href);

const here = dirname(fileURLToPath(import.meta.url));
const loginDir = join(here, '..', 'src', 'main', 'ets', 'login');
const adapterSource = join(loginDir, 'NetBirdAccountCrypto.ets');
const coreSource = join(loginDir, 'NetBirdAccountAuthCore.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-account-crypto-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) { console.log(`     ${detail}`); }
}
function base64url(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('base64').replace(/=/g, '').split('+').join('-')
    .split('/').join('_');
}
interface CryptoSettings {
  randomCalls: number;
  failRandom: boolean;
  failDigest: boolean;
  sha256: (bytes: Uint8Array) => Uint8Array;
}
const settings: CryptoSettings = {
  randomCalls: 0,
  failRandom: false,
  failDigest: false,
  sha256: (bytes: Uint8Array): Uint8Array =>
    new Uint8Array(createHash('sha256').update(bytes).digest())
};
(globalThis as unknown as { __NB_ACCOUNT_CRYPTO: CryptoSettings }).__NB_ACCOUNT_CRYPTO = settings;

try {
  const adapterBytes = readFileSync(adapterSource);
  const adapterFixture = join(dir, 'NetBirdAccountCrypto.ts');
  writeFileSync(adapterFixture, adapterBytes);
  writeFileSync(join(dir, 'NetBirdAccountAuthCore.ts'), readFileSync(coreSource));
  check('adapter fixture is byte-identical to shipped .ets',
    readFileSync(adapterFixture).equals(adapterBytes));
  const code = adapterBytes.toString('utf8');
  check('adapter has no logging or Math.random',
    !code.includes('console.') && !code.includes('hilog.') && !code.includes('Math.random'));
  const adapter = await import(pathToFileURL(adapterFixture).href);

  const vectorVerifier = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
  const vectorChallenge = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';
  const vector = await adapter.computePkceChallenge(vectorVerifier);
  const hostVector = createHash('sha256').update(vectorVerifier, 'ascii').digest('base64url');
  check('RFC 7636 Appendix B challenge matches host SHA-256',
    vector === vectorChallenge && vector === hostVector, vector);

  settings.randomCalls = 0;
  const session = await adapter.createPkceSession('https://client.example.com/auth/callback');
  const expectedVerifier = base64url(Uint8Array.from({ length: 32 }, (_, i) => i));
  const expectedState = Array.from({ length: 32 }, (_, i) => (i + 32).toString(16).padStart(2, '0')).join('');
  const expectedChallenge = createHash('sha256').update(expectedVerifier, 'ascii').digest('base64url');
  check('session uses two secure 32-byte random outputs with expected encodings',
    settings.randomCalls === 2 && session.verifier === expectedVerifier &&
    session.verifier.length === 43 && session.state === expectedState &&
    /^[0-9a-f]{64}$/.test(session.state));
  check('session challenge is S256 base64url without padding and URI is normalized',
    session.challenge === expectedChallenge && session.challenge.length === 43 &&
    /^[A-Za-z0-9_-]{43}$/.test(session.challenge) &&
    session.redirectUri === 'https://client.example.com/auth/callback');

  settings.randomCalls = 0;
  let redirectError = '';
  try {
    await adapter.createPkceSession('http://public.example.com/callback');
  } catch (error) {
    redirectError = error instanceof Error ? error.message : 'non_error';
  }
  check('invalid redirect rejects before requesting random bytes',
    redirectError === 'redirect_http_not_loopback' && settings.randomCalls === 0);

  settings.failRandom = true;
  let randomError = '';
  try {
    await adapter.createPkceSession('https://client.example.com/callback');
  } catch (error) {
    randomError = error instanceof Error ? error.message : 'non_error';
  }
  settings.failRandom = false;
  check('SDK random failure becomes a stable non-secret error code',
    randomError === 'pkce_crypto_unavailable' && !randomError.includes('raw'));

  settings.failDigest = true;
  let digestError = '';
  try {
    await adapter.computePkceChallenge(vectorVerifier);
  } catch (error) {
    digestError = error instanceof Error ? error.message : 'non_error';
  }
  settings.failDigest = false;
  check('SDK digest failure becomes a stable non-secret error code',
    digestError === 'pkce_crypto_unavailable' && !digestError.includes('raw'));
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
