// N13-D1 relay_enabled pass-through self-check (host-side, no device, no
// network, no build).
//
// Same principle as run.mts: the REAL production module
// client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets is
// copied byte-for-byte into an OS-temp fixture (.ts) at run time — the code
// under test is the shipped code, never a hand-held mirror. The file imports
// @ohos.* / libnetbird_core.so, which plain node cannot resolve, so
// relay-flag-hooks.mjs short-circuits ONLY those specifiers to empty stubs
// (registered via node:module register, in-process synchronous hooks are
// unnecessary here).
//
// Asserts the relay_enabled branch of buildConnectorConfigJson +
// parseDeviceConfig:
//   (1) missing/false  -> output JSON has NO 'relay_enabled' key
//   (2) true           -> output JSON has 'relay_enabled': true
//   (3) malformed (string/number) -> treated as false, no throw
//
// Run: node client/entry/selfcheck/run-relay-flag.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { register } from 'node:module';

register(new URL('./relay-flag-hooks.mjs', import.meta.url).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability',
  'NetBirdConnector.ets');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-connector-relay-'));
const fixture: string = join(fixtureDir, 'NetBirdConnector.ts');
writeFileSync(fixture, readFileSync(REAL)); // byte-identical copy of the real module

let passed: number = 0;
let failedCount: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log(`ok   ${name}`);
  } else {
    failedCount++;
    console.log(`FAIL ${name}${detail !== undefined ? ` — ${detail}` : ''}`);
  }
}

// Minimal device config; management_url/private_key are dummy placeholders —
// no real credentials in source (credential discipline).
function device(overrides: Record<string, unknown>): Record<string, unknown> {
  return {
    management_url: 'https://mgmt.example:443',
    private_key: 'DEVICE_PRIVATE_KEY_PLACEHOLDER',
    ...overrides
  };
}

try {
  const mod = await import(pathToFileURL(fixture).href);

  // (1) missing relay_enabled -> key absent
  const absent = JSON.parse(mod.buildConnectorConfigJson(device({}) as never,
    false)) as Record<string, unknown>;
  check('missing flag -> no relay_enabled key',
    !('relay_enabled' in absent), JSON.stringify(absent));

  // (1b) explicit false -> key absent
  const off = JSON.parse(mod.buildConnectorConfigJson(
    device({ relay_enabled: false }) as never, false)) as Record<string, unknown>;
  check('false flag -> no relay_enabled key',
    !('relay_enabled' in off), JSON.stringify(off));

  // (2) true -> relay_enabled: true written
  const on = JSON.parse(mod.buildConnectorConfigJson(
    device({ relay_enabled: true }) as never, false)) as Record<string, unknown>;
  check('true flag -> relay_enabled is true',
    on['relay_enabled'] === true, JSON.stringify(on));

  // (2b) other config keys untouched by the flag
  check('true flag keeps management_url',
    on['management_url'] === 'https://mgmt.example:443', JSON.stringify(on));

  // (3) malformed values -> treated as false, no throw
  for (const bad of ['true', 1, null]) {
    let threw: boolean = false;
    let parsed: Record<string, unknown> = {};
    try {
      parsed = JSON.parse(mod.buildConnectorConfigJson(
        device({ relay_enabled: bad }) as never, false)) as Record<string, unknown>;
    } catch (error) {
      threw = true;
    }
    check(`malformed flag (${JSON.stringify(bad)}) -> no throw and no key`,
      !threw && !('relay_enabled' in parsed), JSON.stringify(parsed));
  }

  // parseDeviceConfig normalizes the flag the same way (no throw on garbage).
  const normOff = mod.parseDeviceConfig(
    JSON.stringify(device({ relay_enabled: 'yes' }))) as Record<string, unknown>;
  check('parseDeviceConfig: malformed flag normalized off',
    normOff['relay_enabled'] === undefined, JSON.stringify(normOff));
  const normOn = mod.parseDeviceConfig(
    JSON.stringify(device({ relay_enabled: true }))) as Record<string, unknown>;
  check('parseDeviceConfig: true kept',
    normOn['relay_enabled'] === true, JSON.stringify(normOn));
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);