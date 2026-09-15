// Relay status log line self-check (host-side, no device, no network, no
// build).
//
// Same principle as run.mts / run-relay-flag.mts: the REAL production module
// client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets is
// copied byte-for-byte into an OS-temp fixture (.ts) at run time — the code
// under test is the shipped code, never a hand-held mirror. The file imports
// @ohos.* / libnetbird_core.so, which plain node cannot resolve, so
// relay-flag-hooks.mjs short-circuits ONLY those specifiers to empty stubs.
//
// Asserts formatRelayStatusLine (the VPN_RELAY_STATUS log line):
//   (1) full status object -> exact line with every new field, correct values
//   (2) relay / relay_advertised missing or null/type-anomalous fields ->
//       no throw, placeholders ('unknown' / -1 / false / none) appear
//   (3) fake token fields carrying a sentinel -> output NEVER contains it
//       (token discipline) nor the url text (urls render as a COUNT only)
//
// Run: node client/entry/selfcheck/run-relay-status-log.mts
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

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-connector-relay-log-'));
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

// Minimal ConnectorStatusResult-shaped object; extra/absent sections are the
// point of the lenient cases, so plain objects (cast at the call) suffice.
function statusBase(): Record<string, unknown> {
  return {
    running: true,
    state: 'connected',
    peer_count: 1,
    route_count: 2,
    reconnects: 0,
    last_error: null,
    session_expiry: '',
    wg_apply_failed: false,
    wg_apply_errors: 0,
    terminal: false,
    signal: { registered: true, reconnects: 0, last_error: null }
  };
}

try {
  const mod = await import(pathToFileURL(fixture).href);

  // (1) full status object -> exact line, every field present and correct
  const full = {
    ...statusBase(),
    relay: {
      enabled: true,
      state: 'connected',
      urls: ['rels://relay-a.example:28443', 'rels://relay-b.example:28443'],
      reconnects: 3,
      frames_tx: 11,
      frames_rx: 22,
      transport_bytes: 33,
      token_valid: true,
      last_error_class: null
    },
    relay_advertised: {
      advertised: true,
      advertised_urls: [
        'rels://adv-a.example:1', 'rels://adv-b.example:2', 'rels://adv-c.example:3'
      ]
    }
  };
  const line1 = mod.formatRelayStatusLine('req-full', full);
  check('full status -> exact relay line',
    line1 ===
      'VPN_RELAY_STATUS|requestId=req-full|enabled=true|state=connected|framesTx=11|' +
      'framesRx=22|transportBytes=33|reconnects=3|tokenValid=true|lastError=none|' +
      'advertised=true|advertisedUrls=3',
    line1);

  // (2) missing sections, null fields, type-anomalous sections -> no throw,
  //     placeholders everywhere (never an exception out of the formatter)
  let threw: boolean = false;
  let lines: string = '';
  try {
    const missing = mod.formatRelayStatusLine('req-missing', statusBase());
    const nullFields = {
      ...statusBase(),
      relay: {
        enabled: null, state: null, urls: null, reconnects: null, frames_tx: null,
        frames_rx: null, transport_bytes: null, token_valid: null, last_error_class: null
      }
    };
    const nulls = mod.formatRelayStatusLine('req-null', nullFields);
    const anomalies = mod.formatRelayStatusLine('req-bad',
      { ...statusBase(), relay: 'garbage', relay_advertised: 7 });
    lines = `${missing}\n${nulls}\n${anomalies}`;
  } catch (error) {
    threw = true;
  }
  check('missing/null/anomalous -> no throw + placeholders',
    !threw &&
    lines.includes('enabled=unknown') && lines.includes('state=unknown') &&
    lines.includes('framesTx=-1') && lines.includes('framesRx=-1') &&
    lines.includes('transportBytes=-1') && lines.includes('reconnects=-1') &&
    lines.includes('tokenValid=false') &&
    lines.includes('lastError=unknown') && lines.includes('lastError=none') &&
    lines.includes('advertised=false') &&
    lines.includes('advertisedUrls=-1'),
    lines);

  // (3) token discipline: sentinel-bearing fake token fields and a sentinel
  //     url must never appear in the output — urls render as a COUNT only
  const SECRET_TOKEN = 'SENTINEL-TOKEN-DO-NOT-LEAK-9f2c';
  const SECRET_SIG = 'SENTINEL-SIG-DO-NOT-LEAK-01ab';
  const SECRET_URL = 'rels://secret-relay-host.example:29999';
  const leaky = {
    ...statusBase(),
    relay: {
      enabled: true, state: 'connected', urls: [SECRET_URL], reconnects: 1,
      frames_tx: 1, frames_rx: 1, transport_bytes: 1, token_valid: true,
      last_error_class: null,
      token_payload: SECRET_TOKEN,
      token_signature: SECRET_SIG
    },
    relay_advertised: { advertised: true, advertised_urls: [SECRET_URL] }
  };
  const line3 = mod.formatRelayStatusLine('req-leak', leaky);
  check('token material and url text never leak',
    !line3.includes(SECRET_TOKEN) && !line3.includes(SECRET_SIG) &&
    !line3.includes(SECRET_URL) && line3.includes('advertisedUrls=1'),
    line3);
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);