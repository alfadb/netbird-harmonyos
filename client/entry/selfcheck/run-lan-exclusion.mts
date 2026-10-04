// 动态基础 LAN 排除 self-check (scenario-A roaming fix,
// LAN-ROUTE-ROAMING-ANALYSIS.md §4 step 1) — host-side, no device.
//
// The production module client/entry/src/main/ets/vpnextensionability/
// NetBirdEndpointExclusion.ets is a zero-import pure module; like run.mts,
// this harness copies the REAL file byte-for-byte into an OS-temp fixture
// (.ts) and drives the shipped deriveLanExclusion through the completion
// criteria's three probe shapes (home Wi-Fi -> the retired static exclusion
// verbatim; cellular -> none; nothing usable -> none), plus wiring pins on
// the shipped sources (the static 192.168.50.0/24 is gone, both config
// assembly legs derive dynamically).
//
// Run: node client/entry/selfcheck/run-lan-exclusion.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
const GW = '10.99.0.1';

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-lan-exclusion-'));
const fixture: string = join(fixtureDir, 'NetBirdEndpointExclusion.ts');
writeFileSync(fixture, readFileSync(join(REAL_DIR, 'NetBirdEndpointExclusion.ets')));

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

try {
  const m = await import(pathToFileURL(fixture).href);

  // ---- the completion criteria's three shapes --------------------------------
  const wifi = m.deriveLanExclusion({
    bearerTypes: [1], // NetBearType.BEARER_WIFI
    linkAddresses: [{ address: '192.168.50.199', prefixLength: 24 }]
  }, GW);
  check('Wi-Fi 192.168.50.x/24 -> the retired static exclusion verbatim',
    wifi.entry !== null &&
    JSON.stringify(wifi.entry) === JSON.stringify({
      destination: '192.168.50.0', prefixLength: 24, gateway: GW,
      hasGateway: true, isDefaultRoute: false, isExcludedRoute: true
    }) && wifi.reason === 'lan' && wifi.network === '192.168.50.0',
    JSON.stringify(wifi));

  const cellular = m.deriveLanExclusion({
    bearerTypes: [0], // NetBearType.BEARER_CELLULAR
    linkAddresses: [{ address: '10.41.63.59', prefixLength: 29 }]
  }, GW);
  check('cellular (10.x local prefix) -> NO LAN exclusion',
    cellular.entry === null && cellular.reason === 'cellular' &&
    cellular.network === '', JSON.stringify(cellular));
  check('BEARER_CELLULAR mirrors NetBearType.BEARER_CELLULAR (0)',
    m.BEARER_CELLULAR === 0);

  const empty = m.deriveLanExclusion(m.emptyLanExclusionProbe(), GW);
  check('empty probe (no physical net / read failed) -> NO LAN exclusion',
    empty.entry === null && empty.reason === 'no-usable-link-address',
    JSON.stringify(empty));

  // ---- masking + edge prefixes ----------------------------------------------
  check('host bits are masked into the network address',
    m.deriveLanExclusion({ bearerTypes: [1],
      linkAddresses: [{ address: '172.20.9.77', prefixLength: 12 }] }, GW)
      .network === '172.16.0.0' &&
    m.deriveLanExclusion({ bearerTypes: [1],
      linkAddresses: [{ address: '10.41.63.59', prefixLength: 29 }] }, GW)
      .network === '10.41.63.56');
  check('IPv6 beside the IPv4 is skipped, the IPv4 wins',
    m.deriveLanExclusion({ bearerTypes: [1], linkAddresses: [
      { address: 'fe80::1', prefixLength: 64 },
      { address: '192.168.50.240', prefixLength: 24 }
    ] }, GW).network === '192.168.50.0');
  check('IPv6-only link addresses -> NO LAN exclusion',
    m.deriveLanExclusion({ bearerTypes: [1],
      linkAddresses: [{ address: 'fe80::1', prefixLength: 64 }] }, GW).entry === null);
  check('prefix 0 would exclude 0.0.0.0/0 — refused',
    m.deriveLanExclusion({ bearerTypes: [1],
      linkAddresses: [{ address: '192.168.50.199', prefixLength: 0 }] }, GW).entry === null);
  check('out-of-range / non-integer / absent prefixes are refused, next entry can win',
    m.deriveLanExclusion({ bearerTypes: [1], linkAddresses: [
      { address: '192.168.50.199', prefixLength: 33 },
      { address: '192.168.50.200', prefixLength: 24.5 },
      { address: '192.168.50.201' },
      { address: '192.168.50.202', prefixLength: 24 }
    ] }, GW).network === '192.168.50.0');
  check('non-IPv4 literals never yield an entry',
    m.deriveLanExclusion({ bearerTypes: [1], linkAddresses: [
      { address: '192.168.50.199:53', prefixLength: 24 },
      { address: 'not-an-ip', prefixLength: 24 },
      { address: '999.1.1.1', prefixLength: 24 }
    ] }, GW).entry === null);
  check('a probe of null/undefined never throws and yields none',
    m.deriveLanExclusion(null, GW).entry === null &&
    m.deriveLanExclusion(undefined, GW).reason === 'no-usable-link-address');
  check('gateway rides the entry verbatim (base template tunnel address)',
    m.deriveLanExclusion({ bearerTypes: [1],
      linkAddresses: [{ address: '192.168.50.199', prefixLength: 24 }] },
    '100.108.144.165').entry?.gateway === '100.108.144.165');

  // ---- merged config keeps the derived entry like the old static one --------
  const merged = m.mergeExcludedRouteEntries([
    { destination: '10.99.0.0', prefixLength: 24, gateway: GW, hasGateway: true,
      isDefaultRoute: false, isExcludedRoute: false }
  ], wifi.entry !== null ? [wifi.entry] : []);
  check('mergeExcludedRouteEntries carries the derived LAN entry unchanged',
    merged.length === 2 && merged[1] === wifi.entry, JSON.stringify(merged));

  // ---- wiring pins on the shipped sources -----------------------------------
  const cfgSource: string =
    readFileSync(join(REAL_DIR, 'NetBirdVpnConfig.ets'), 'utf8');
  const extSource: string =
    readFileSync(join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const connSource: string =
    readFileSync(join(REAL_DIR, 'NetBirdConnector.ets'), 'utf8');
  check('the static 192.168.50.0/24 base exclusion is GONE from the config module',
    cfgSource.indexOf("destination: '192.168.50.0'") < 0 &&
    cfgSource.indexOf('isExcludedRoute: true') < 0,
    'NetBirdVpnConfig.ets still carries a static excluded route');
  check('the extension derives the LAN exclusion on BOTH assembly legs',
    extSource.indexOf('deriveLanExclusion(') > 0 &&
    extSource.split("this.lanExclusionForConfig(recreateTemplate.tunnelAddress, 'recreate')")
      .length === 2 &&
    extSource.split("this.lanExclusionForConfig(baseTemplate.tunnelAddress, 'initial')")
      .length === 2,
    'initial/recreate call-site pin');
  check('the extension logs the derivation (reason/network/prefix for device truth)',
    extSource.indexOf('VPN_LAN_EXCLUSION|requestId=') > 0 &&
    extSource.indexOf('linkAddresses=%{public}d') > 0);
  check('the collector reuses the physical-default selection (B5 handle rule)',
    connSource.indexOf(
      'export function physicalDefaultLanProbe(): LanExclusionProbe {') > 0 &&
    connSource.indexOf('const netId: number = physicalDefaultNetId();',
      connSource.indexOf('physicalDefaultLanProbe')) > 0);
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log('');
console.log(`${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);
