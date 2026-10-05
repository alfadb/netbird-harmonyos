// Explicit-logout flow — shell self-check (host-only, no device, no network,
// no build). Pins the WIRING and the CALL ORDER of the logout channel on the
// byte-identical shipped sources, because the flow spans the UI page (writes
// a one-shot ticket), the VPN extension process (the only one holding the
// live connector: sends the ONE management Logout, answers, then tears the
// session down) and the Rust core (`connector_logout` exists, `stop()` never
// logs out). The core-side BEHAVIOUR (stop sends no Logout, explicit logout
// fires exactly once) is pinned by the Rust integration tests
// (client/core/tests/connector.rs); this file pins the entry-side order:
//
//   UI:      vpn-live? --yes--> ticket -> bounded wait for the answer
//                             \-no----> local delete + console hint
//   UI:      logout attempt ALWAYS before the credential deletion
//   EXT:     connectorLogout() -> result file -> watcher stop ->
//            connectorStop() -> destroyOnce()
//
// Run: node client/entry/selfcheck/run-logout-flow.mts
// Exit 0 = all assertions pass.

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const PAGE = readFileSync(join(ETS, 'pages', 'Index.ets'), 'utf8');
const ABILITY = readFileSync(join(ETS, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets'), 'utf8');
const CONNECTOR_ETS = readFileSync(join(ETS, 'vpnextensionability', 'NetBirdConnector.ets'), 'utf8');
const CORE_CONNECTOR = readFileSync(join(HERE, '..', '..', 'core', 'src', 'connector.rs'), 'utf8');

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}
function sliceFrom(src: string, begin: string, end: string): string {
  const at = src.indexOf(begin);
  if (at < 0) { return ''; }
  const stop = src.indexOf(end, at);
  return stop < 0 ? src.slice(at) : src.slice(at, stop);
}

// ---- the flow's own slices ------------------------------------------------

// UI: performLogout .. waitForLogoutResult (the attempt half)
const attemptHalf = sliceFrom(PAGE, 'private performLogout(): void', 'private readLogoutResult(): boolean | null');
// UI: finishLogout (the deletion half)
const deletionHalf = sliceFrom(PAGE, 'private finishLogout(serverLogoutConfirmed: boolean): void', 'private removeSetupFile(name: string): void');
// EXT: pollLogoutTicket .. the next method
const ticketHandler = sliceFrom(ABILITY, 'private pollLogoutTicket(): void', 'private pollN2AEvent(): void');
// EXT: the watcher tick body (pollLogoutTicket rides before the status read)
const watcherTick = sliceFrom(ABILITY, 'this.watcherTimer = setInterval((): void => {', '}, STATUS_POLL_MS);');
// CORE: stop() body (between the stop fn and the stop_json doc)
const stopBody = sliceFrom(CORE_CONNECTOR, 'pub fn stop(&self) -> StopOutcome {', '/// The `connector_stop()` JSON document');

// ---- core: stop never logs out; the explicit entry exists -----------------

check('core stop() sends NO management Logout on any stop path',
  stopBody.length > 0 &&
  !stopBody.includes('.logout()') &&
  stopBody.includes('NO Logout here'));
check('core has the explicit logout entry (connector_logout_json -> logout_json)',
  CORE_CONNECTOR.includes('pub fn connector_logout_json() -> String') &&
  CORE_CONNECTOR.includes('pub fn logout_json(&self) -> String') &&
  CORE_CONNECTOR.includes('EXPLICIT_LOGOUT_WAIT_MS'));
check('NAPI exports connector_logout',
  readFileSync(join(HERE, '..', '..', 'core', 'src', 'napi.rs'), 'utf8')
    .includes('reg!("connector_logout", napi_connector_logout);'));

// ---- shell wrapper --------------------------------------------------------

check('NetBirdConnector.ets wraps core.connector_logout',
  CONNECTOR_ETS.includes('export function connectorLogout(): ConnectorLogoutResult') &&
  CONNECTOR_ETS.includes('core.connector_logout()'));

// ---- UI order: attempt BEFORE the credential deletion ----------------------

check('UI performLogout gates on the live VPN state (vpnActiveState)',
  attemptHalf.includes('vpnActiveState(this.published.state)') &&
  attemptHalf.includes('UI_LOGOUT_LOCAL_ONLY|reason=vpn-not-running'));
check('UI: the logout ticket is written BEFORE any credential file is deleted',
  attemptHalf.length > 0 && deletionHalf.length > 0 &&
  attemptHalf.indexOf('this.writeLogoutTicket();') >= 0 &&
  deletionHalf.indexOf('this.removeSetupFile(SETUP_CONFIG_FILE);') >= 0 &&
  PAGE.indexOf('private performLogout(): void') < PAGE.indexOf('this.writeLogoutTicket();') &&
  PAGE.indexOf('this.writeLogoutTicket();') < PAGE.indexOf('this.removeSetupFile(SETUP_CONFIG_FILE);'),
  'order');
check('UI: the answer wait is bounded and only the matched ticket_ms counts',
  attemptHalf.includes('await sleep(LOGOUT_POLL_MS)') &&
  PAGE.includes('const LOGOUT_WAIT_MS: number = 15000;') &&
  PAGE.includes('parsed.ticket_ms !== this.logoutTicketMs'));
check('UI: a not-confirmed logout keeps deleting locally and points at the console',
  deletionHalf.includes('this.removeSetupFile(SETUP_CONFIG_FILE);') &&
  deletionHalf.includes('this.removeSetupFile(SETUP_KEY_FILE);') &&
  deletionHalf.includes('如需从管理端移除本设备，请在管理后台删除'));
check('UI never calls the native logout directly (ticket channel only)',
  !PAGE.includes('core.connector_logout'));

// ---- EXT order: logout -> answer -> teardown ------------------------------

check('EXT ticket handler: ONE connectorLogout() BEFORE the result file, the watcher stop, connectorStop() and destroyOnce',
  ticketHandler.length > 0 &&
  ticketHandler.indexOf('connectorLogout();') >= 0 &&
  ticketHandler.indexOf('connectorLogout();') < ticketHandler.indexOf('${LOGOUT_RESULT_FILE_NAME}') &&
  ticketHandler.indexOf('${LOGOUT_RESULT_FILE_NAME}') < ticketHandler.indexOf('this.stopStatusAndNetcfgWatcher();') &&
  ticketHandler.indexOf('this.stopStatusAndNetcfgWatcher();') < ticketHandler.indexOf('const stop = connectorStop();') &&
  ticketHandler.indexOf('const stop = connectorStop();') < ticketHandler.indexOf("this.destroyOnce('explicit-logout')"),
  'order');
check('EXT ticket handler: the Logout attempt is best-effort (try/catch, never blocks the teardown)',
  ticketHandler.includes("reason = 'native-throw';") &&
  ticketHandler.includes('cause=explicit-logout'));
check('EXT ticket is consumed exactly once and freshness-bounded',
  ticketHandler.includes('parseLogoutTicketIssuedAtMs(text)') &&
  ticketHandler.includes('reason=stale-ticket') &&
  ABILITY.includes('const LOGOUT_TICKET_MAX_AGE_MS: number = 30000;') &&
  ticketHandler.lastIndexOf('fs.unlinkSync(path);') > ticketHandler.indexOf('connectorLogout();'));
check('EXT watcher tick polls the logout ticket BEFORE the status read',
  watcherTick.length > 0 &&
  watcherTick.indexOf('this.pollLogoutTicket();') >= 0 &&
  watcherTick.indexOf('this.pollLogoutTicket();') < watcherTick.indexOf('const status: ConnectorStatusResult | null = pollStatusOrNull();'));

console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
