// Fakes only platform boundaries; the controller fixture stays byte-identical.
const CORE = 'data:text/javascript,' + encodeURIComponent(
  'function s() { return globalThis.__NB_ACCOUNT_LOGIN; }\n' +
  'export default {' +
  ' account_auth_discovery_start(text) { const x=s(); x.requests.push(JSON.parse(text));' +
  ' if (x.startError) return JSON.stringify({ok:false,error:"private detail"});' +
  ' x.requestId++; return JSON.stringify({ok:true,request_id:x.requestId}); },' +
  ' account_auth_discovery_poll(id) { const x=s(); x.pollIds.push(id);' +
  ' if (x.pollWait) return JSON.stringify({state:"pending"});' +
  ' const next=x.polls.length ? x.polls.shift() : {state:"done",config:x.metadata};' +
  ' return JSON.stringify(next); },' +
  ' account_auth_discovery_cancel(id) { s().cancelled.push(id); return "{\\"ok\\":true}"; }' +
  '};');
const CRYPTO = 'data:text/javascript,' + encodeURIComponent(
  'export async function createPkceSession(uri) {' +
  ' const x=globalThis.__NB_ACCOUNT_LOGIN; if (x.cryptoWait) await x.cryptoWait;' +
  ' return {state:"secure-state",verifier:"A".repeat(43),challenge:"B".repeat(43),redirectUri:uri}; }');
const TRANSPORT = 'data:text/javascript,' + encodeURIComponent(
  'export class AccountTokenTransport {' +
  ' async exchangeAuthorizationCode(config, session, code) {' +
  ' const x=globalThis.__NB_ACCOUNT_LOGIN; x.exchanges.push({config,session,code});' +
  ' if (x.exchangeWait) return await x.exchangeWait;' +
  ' return x.tokenResult; }' +
  ' cancel() { const x=globalThis.__NB_ACCOUNT_LOGIN; x.transportCancels++;' +
  ' if (x.cancelExchange) x.cancelExchange(); } }');
const STUBS = new Map([
  ['libnetbird_core.so', CORE],
  ['./NetBirdAccountCrypto', CRYPTO],
  ['./NetBirdAccountTransport', TRANSPORT]
]);
export function resolve(specifier, context, next) {
  if (STUBS.has(specifier)) return { url: STUBS.get(specifier), shortCircuit: true };
  if (specifier === './NetBirdAccountAuthCore' && context.parentURL !== undefined) {
    return { url: new URL('NetBirdAccountAuthCore.ts', new URL('.', context.parentURL)).href,
      shortCircuit: true };
  }
  return next(specifier, context);
}
