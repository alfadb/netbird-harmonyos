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
  check('default management url requires a user-supplied server',
    mod.DEFAULT_MANAGEMENT_URL === '');
  check('config file name matches the --ps channel / extension reader',
    mod.SETUP_CONFIG_FILE === 'netbird-device-config.json');
  check('setup key file name matches the --ps channel / extension reader',
    mod.SETUP_KEY_FILE === 'netbird-setup-key');

  // --- management URL validation + normalization ---
  const cases: Array<[string, string, string]> = [
    ['https://management.example.test', 'https://management.example.test', 'plain https url'],
    ['  https://management.example.test:443  ', 'https://management.example.test:443', 'surrounding whitespace trimmed'],
    ['https://management.example.test/', 'https://management.example.test', 'trailing slash stripped'],
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
    ['management.example.test', 'missing scheme'],
    ['//management.example.test', 'scheme without name'],
    ['http://management.example.test', 'http scheme'],
    ['HTTP://management.example.test', 'uppercase http scheme'],
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

  // Execute the page's unchanged methods with in-memory storage and side-effect
  // counters. This is host coverage, not a fresh-install/device UI acceptance.
  const page = readFileSync(join(here, '..', 'src', 'main', 'ets', 'pages', 'Index.ets'), 'utf8');
  function pageMethod(begin: string, end: string): string {
    const start = page.indexOf(begin);
    const stop = page.indexOf(end, start);
    if (start < 0 || stop < 0) { throw new Error('page method boundary missing'); }
    return page.slice(start, stop);
  }
  check('first-run input and logout reset both use the default constant',
    page.includes('@State urlInput: string = DEFAULT_MANAGEMENT_URL;') &&
    page.includes('this.urlInput = DEFAULT_MANAGEMENT_URL;'));
  check('generic server placeholder stays available',
    page.includes('服务器地址（https://…）'));
  const uiFixture = join(dir, 'SetupPageHarness.ts');
  writeFileSync(uiFixture, `${readFileSync(source, 'utf8')}
const ACCOUNT_MODE_SETUP_KEY = 'setup-key';
const ACCOUNT_MODE_ACCOUNT = 'account';
const ACCOUNT_MODE_FILE_NAME = 'auth-mode';
const ACCOUNT_INTENT_SETUP = 'setup';
const getContext = (page: any) => ({ filesDir: '/fixture' });
const files = new Map<string, string>();
const fs = {
  accessSync: (path: string) => files.has(path),
  readTextSync: (path: string) => files.get(path) ?? ''
};
const parseAuthMode = (text: string) => text.trim();
export class SetupPageHarness {
  configured = false; configuredUrl = ''; urlInput = DEFAULT_MANAGEMENT_URL;
  authMode = ACCOUNT_MODE_SETUP_KEY; keyInput = 'synthetic-test-key';
  accountLoginBusy = false; accountLoginCommitting = false;
  writes = 0; starts = 0; discoveries = 0;
  accountModeSwitchBlocked() { return false; }
  devicePrivateKey() { return 'synthetic-test-identity'; }
  deviceRelayEnabled() { return true; }
  writeSetupFile(name: string, text: string) {
    this.writes++; files.set('/fixture/' + name, text); return true;
  }
  requestStart() { this.starts++; }
  cancelAccountLogin(value: boolean) {}
  async openAccountLogin(intent: string, url: string, extend: boolean) { this.discoveries++; }
  storedConfig() { return files.get('/fixture/' + SETUP_CONFIG_FILE) ?? ''; }
  ${pageMethod('private refreshSetupState(): void', '// Save the form:')}
  ${pageMethod('private saveSetup(): void', 'private beginConfigurationEdit(): void')}
  ${pageMethod('private beginConfigurationEdit(): void', 'private selectAuthMode(')}
  ${pageMethod('private async beginAccountLogin(): Promise<void>', '// The ONE ArkWeb PKCE surface opener')}
}
`);
  const uiMod = await import(pathToFileURL(uiFixture).href);
  const ui = new uiMod.SetupPageHarness();
  ui.saveSetup();
  await ui.beginAccountLogin();
  check('empty default cannot save config or start VPN', ui.writes === 0 && ui.starts === 0);
  check('empty default cannot reach account discovery', ui.discoveries === 0);
  check('empty default produces a readable error', hasCjk(ui.formError));
  const userUrl = 'https://user-managed.example.test:8443';
  ui.urlInput = userUrl;
  ui.saveSetup();
  const savedConfig = ui.storedConfig();
  check('user server saves and reads back despite empty factory default',
    ui.writes === 3 && ui.starts === 1 && ui.configured && ui.configuredUrl === userUrl &&
    mod.readStoredManagementUrl(savedConfig) === userUrl);
  ui.urlInput = mod.DEFAULT_MANAGEMENT_URL;
  ui.refreshSetupState();
  check('loading existing config preserves its original server and bytes',
    ui.configuredUrl === userUrl && ui.storedConfig() === savedConfig && ui.writes === 3);
  ui.beginConfigurationEdit();
  check('editing existing config starts from the saved user server', ui.urlInput === userUrl);
  ui.urlInput = 'https://unsaved.example.test';
  ui.cancelConfigurationEdit();
  check('canceling edit restores saved server without rewriting config',
    ui.urlInput === userUrl && ui.configuredUrl === userUrl &&
    ui.storedConfig() === savedConfig && ui.writes === 3);
  ui.beginConfigurationEdit();
  ui.keyInput = 'synthetic-test-key';
  ui.saveSetup();
  check('saving unchanged server after editing preserves original config bytes',
    ui.configuredUrl === userUrl && ui.storedConfig() === savedConfig && ui.writes === 6);

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
