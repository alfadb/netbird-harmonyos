// Host-side checks against the byte-identical first-run setup pure logic
// (validation + credential file content construction).
// Run: node client/entry/selfcheck/run-setup-config.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', 'src', 'main', 'ets', 'setup', 'NetBirdSetupConfig.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-setup-config-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}
const hasCjk = (s: string): boolean => /[\u4e00-\u9fff]/.test(s);
try {
  const fixture = join(dir, 'NetBirdSetupConfig.ts');
  writeFileSync(fixture, readFileSync(source));
  const mod = await import(pathToFileURL(fixture).href);

  // --- constants stay on the SHARED credential protocol (--ps channel) ---
  check('default management url is the project deployment',
    mod.DEFAULT_MANAGEMENT_URL === 'https://api.netcenter.alfadb.cn');
  check('config file name matches the --ps channel / extension reader',
    mod.SETUP_CONFIG_FILE === 'netbird-device-config.json');
  check('setup key file name matches the --ps channel / extension reader',
    mod.SETUP_KEY_FILE === 'netbird-setup-key');

  // --- management URL validation + normalization ---
  const cases: Array<[string, string, string]> = [
    ['https://api.netcenter.alfadb.cn', 'https://api.netcenter.alfadb.cn', 'plain https url'],
    ['  https://api.netbird.io:443  ', 'https://api.netbird.io:443', 'surrounding whitespace trimmed'],
    ['https://api.netcenter.alfadb.cn/', 'https://api.netcenter.alfadb.cn', 'trailing slash stripped'],
    ['https://API.NetCenter.Example:443/', 'https://api.netcenter.example:443', 'scheme+host lowercased, port kept'],
    ['https://netbird.example.com:443/api?x=1', 'https://netbird.example.com:443', 'path and query stripped']
  ];
  for (const [raw, want, label] of cases) {
    const got = mod.checkManagementUrl(raw);
    check(`accept ${label}`, got.ok && got.url === want && got.error === '');
  }
  const bad: Array<[string, string]> = [
    ['', 'empty input'],
    ['   ', 'whitespace-only input'],
    ['api.netcenter.alfadb.cn', 'missing scheme'],
    ['//api.netcenter.alfadb.cn', 'scheme without name'],
    ['http://api.netcenter.alfadb.cn', 'http scheme'],
    ['HTTP://api.netcenter.alfadb.cn', 'uppercase http scheme'],
    ['ftp://h.example', 'ftp scheme'],
    ['https://', 'empty host'],
    ['https:///api', 'empty host before path'],
    ['https://a b.example', 'space inside host'],
    ['https://user@h.example', 'userinfo in authority'],
    ['https://h.example:abc', 'non-numeric port'],
    ['https://h.example:', 'empty port'],
    ['https://h.example:0', 'port 0'],
    ['https://h.example:70000', 'port above 65535']
  ];
  for (const [raw, label] of bad) {
    const got = mod.checkManagementUrl(raw);
    check(`reject ${label}`, !got.ok && got.url === '');
    if (!got.ok) {
      check(`reject ${label} has a Chinese readable error`,
        got.error.length > 0 && hasCjk(got.error));
    }
  }

  // --- setup key validation (non-empty hard, UUID shape loose hint) ---
  check('empty setup key rejected', !mod.checkSetupKey('').ok);
  check('whitespace-only setup key rejected', !mod.checkSetupKey('   ').ok);
  const dashed = mod.checkSetupKey('6dc45f77-5f0d-4a0f-8a75-9df5b6a04bbb');
  check('dashed uuid key accepted without hint', dashed.ok && dashed.hint === '' && dashed.error === '');
  const dashless = mod.checkSetupKey('6DC45F775F0D4A0F8A759DF5B6A04BBB');
  check('dashless uppercase hex key accepted without hint', dashless.ok && dashless.hint === '');
  const odd = mod.checkSetupKey('my-key-2026');
  check('odd-looking key still accepted (loose)', odd.ok && odd.error === '');
  check('odd-looking key gets a Chinese hint', odd.hint.length > 0 && hasCjk(odd.hint));
  const nearMiss = mod.checkSetupKey('6dc45f77-5f0d-4a0f-8a75-9df5b6a04bbg');
  check('non-hex uuid-shaped key gets a hint, not a refusal', nearMiss.ok && nearMiss.hint.length > 0);

  // --- credential file content construction (same shapes as --ps channel) ---
  const cfgText = mod.buildDeviceConfigJson('https://m.example:443', 'AAECAwQFBgcICQ==');
  const cfg = JSON.parse(cfgText) as Record<string, unknown>;
  check('device config json keeps management_url', cfg['management_url'] === 'https://m.example:443');
  check('device config json keeps private_key', cfg['private_key'] === 'AAECAwQFBgcICQ==');
  check('device config json has the three written fields only',
    Object.keys(cfg).length === 3 && cfg['management_url'] !== undefined &&
    cfg['private_key'] !== undefined && cfg['relay_enabled'] !== undefined);
  check('fresh configure defaults relay_enabled to true',
    cfg['relay_enabled'] === true);
  const cfgOff = JSON.parse(mod.buildDeviceConfigJson('https://m.example:443',
    'AAECAwQFBgcICQ==', false)) as Record<string, unknown>;
  check('explicit relayEnabled=false is written through',
    cfgOff['relay_enabled'] === false);
  check('setup key file text matches the --ps channel shape',
    mod.buildSetupKeyFileText('abc-123') === 'abc-123\n');
  check('setup key file text trims surrounding whitespace',
    mod.buildSetupKeyFileText('  abc-123 \n') === 'abc-123\n');

  // --- stored relay switch read-back (reconfigure preservation) ---
  const storedText = '{"management_url":"https://x.example","private_key":"pk-1"}';
  check('stored relay: missing flag reads back true',
    mod.readStoredRelayEnabled(storedText) === true);
  check('stored relay: explicit false reads back false',
    mod.readStoredRelayEnabled('{"management_url":"https://x.example","relay_enabled":false}') === false);
  check('stored relay: explicit true reads back true',
    mod.readStoredRelayEnabled('{"management_url":"https://x.example","relay_enabled":true}') === true);
  check('stored relay: malformed flag reads back true',
    mod.readStoredRelayEnabled('{"management_url":"https://x.example","relay_enabled":"no"}') === true);
  check('stored relay: garbage config text reads back true',
    mod.readStoredRelayEnabled('{oops') === true);
  // The reconfigure round-trip the page performs: read the old value, feed
  // it into the rewrite — an explicit opt-out survives, a flagless legacy
  // config upgrades to the enabled default.
  const reoff = JSON.parse(mod.buildDeviceConfigJson('https://x.example', 'pk-1',
    mod.readStoredRelayEnabled('{"management_url":"https://x.example","private_key":"pk-1","relay_enabled":false}'))) as Record<string, unknown>;
  check('reconfigure preserves an explicit relay_enabled:false',
    reoff['relay_enabled'] === false);
  const reon = JSON.parse(mod.buildDeviceConfigJson('https://x.example', 'pk-1',
    mod.readStoredRelayEnabled(storedText))) as Record<string, unknown>;
  check('reconfigure of a flagless legacy config writes relay_enabled:true',
    reon['relay_enabled'] === true);

  // --- stored config read-back (display + identity reuse) ---
  check('stored management url read back', mod.readStoredManagementUrl(storedText) === 'https://x.example');
  check('stored private key read back', mod.readStoredPrivateKey(storedText) === 'pk-1');
  check('garbage config text yields empty url', mod.readStoredManagementUrl('{oops') === '');
  check('garbage config text yields empty private key', mod.readStoredPrivateKey('{oops') === '');
  check('config without management url yields empty url',
    mod.readStoredManagementUrl('{"private_key":"k"}') === '');
  check('config without private key yields empty private key',
    mod.readStoredPrivateKey('{"management_url":"https://x"}') === '');
  check('empty management url field yields empty url',
    mod.readStoredManagementUrl('{"management_url":"","private_key":"k"}') === '');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
