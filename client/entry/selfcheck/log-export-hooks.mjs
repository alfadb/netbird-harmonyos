// Loader-side resolve hook for run-log-export.mts ONLY (host-side fixture).
// Same principle as the other hooks in this directory: the REAL production
// module (NetBirdLogExport.ets) is copied byte-for-byte into an OS-temp .ts
// fixture at run time and driven end-to-end; only the platform seams are
// replaced, and each replacement is FUNCTIONAL:
//   - @ohos.file.fs       -> log-export-fs-stub.mjs (path-aware in-memory fs)
//   - @ohos.file.picker   -> log-export-picker-stub.mjs (3 real picker answers)
//   - @ohos.hilog         -> a recording formatter: every line lands in
//                            globalThis.__NB_LOG with its %{public}* arguments
//                            substituted, so the suite can assert both the
//                            markers AND that no path/URI is ever logged.
//   - @ohos.app.ability.common -> an empty default export; the module under
//                            test imports it for the UIAbilityContext TYPE
//                            only, and the fixture runner passes its own
//                            context object (filesDir).
//
// No module under test is ever rewritten.

const STUBS = {
  '@ohos.file.fs': './log-export-fs-stub.mjs',
  '@ohos.file.picker': './log-export-picker-stub.mjs'
};

const HILOG_SRC = [
  'const lines = () => (globalThis.__NB_LOG !== undefined ? globalThis.__NB_LOG' +
    ' : (globalThis.__NB_LOG = []));',
  'function fmt(format, args) {',
  '  let i = 0;',
  '  return String(format).replace(/%[{]public[}][sd]/g, () => {',
  '    const v = args[i]; i += 1;',
  '    return v === undefined ? "undefined" : String(v);',
  '  });',
  '}',
  'const hilog = {',
  '  info(d, t, f, ...a) { lines().push(fmt(f, a)); },',
  '  warn(d, t, f, ...a) { lines().push("WARN " + fmt(f, a)); },',
  '  error(d, t, f, ...a) { lines().push("ERROR " + fmt(f, a)); }',
  '};',
  'export default hilog;'
].join('\n');

export function resolve(specifier, context, next) {
  if (specifier === '@ohos.hilog') {
    return { url: 'data:text/javascript,' + encodeURIComponent(HILOG_SRC), shortCircuit: true };
  }
  if (specifier === '@ohos.app.ability.common') {
    return {
      url: 'data:text/javascript,' + encodeURIComponent('export default {};'),
      shortCircuit: true
    };
  }
  if (Object.prototype.hasOwnProperty.call(STUBS, specifier)) {
    return { url: new URL(STUBS[specifier], import.meta.url).href, shortCircuit: true };
  }
  return next(specifier, context);
}
