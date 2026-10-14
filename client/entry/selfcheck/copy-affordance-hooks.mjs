// SDK-module loader fakes for run-copy-affordance.mts ONLY (host-side fixture).
// Same principle as keep-alive-hooks.mjs / route-apply-hooks.mjs: the REAL
// production module is staged byte-for-byte into one OS-temp directory and driven
// under plain node, and only the @ohos.* platform seam is resolved here.
//
//   - @ohos.pasteboard -> a RECORDING stub of the public write path: createData
//     records every (mimeType, value) pair it is handed and returns a PasteData
//     look-alike, and getSystemPasteboard().setDataSync records the object it is
//     written. That is how the test asserts the EXACT string the app puts on the
//     clipboard — through the shipped module's own call path, not a mirror of it.
//     Nothing else of the namespace exists, so a stray read API in the production
//     module would fail here loudly instead of passing silently.
//
// No module under test is ever rewritten.

import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const PASTEBOARD = 'data:text/javascript,' + encodeURIComponent(
  'const s = () => globalThis.__NB_COPY;\n' +
  'const api = {\n' +
  ' MIMETYPE_TEXT_PLAIN: "text/plain",\n' +
  ' createData(mimeType, value) {\n' +
  '  s().created.push({ mimeType: mimeType, value: value });\n' +
  '  return { mimeType: mimeType, value: value }; },\n' +
  ' getSystemPasteboard() {\n' +
  '  return { setDataSync(data) { s().written.push(data); } }; }\n' +
  '};\nexport default api;');

const STUBS = new Map([
  ['@ohos.pasteboard', PASTEBOARD]
]);

// The staged production module may import its siblings the way ArkTS does — with no
// file extension — which node's ESM resolver would refuse. The staged directory is
// the only place this rule applies.
export function resolve(specifier, context, next) {
  const url = STUBS.get(specifier);
  if (url !== undefined) {
    return { url: url, shortCircuit: true };
  }
  if (specifier.startsWith('.') && typeof context.parentURL === 'string' &&
    context.parentURL.startsWith('file:')) {
    try {
      const candidate = join(dirname(fileURLToPath(context.parentURL)), specifier + '.ts');
      if (existsSync(candidate)) {
        return { url: pathToFileURL(candidate).href, shortCircuit: true };
      }
    } catch (ignored) {
      // fall through to the default resolver
    }
  }
  return next(specifier, context);
}
