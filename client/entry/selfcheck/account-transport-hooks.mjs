// Functional host fake for the production AccountTokenTransport fixture.
const HTTP = 'data:text/javascript,' + encodeURIComponent(
  'function state() { return globalThis.__NB_HTTP; }\n' +
  'class Request {\n' +
  '  constructor() { this.destroyCount = 0; state().requests.push(this); }\n' +
  '  request(url, options) {\n' +
  '    this.url = url; this.options = options;\n' +
  '    const s = state();\n' +
  '    if (s.reply instanceof Promise) { return s.reply; }\n' +
  '    if (s.reject) { return Promise.reject(new Error("private platform error")); }\n' +
  '    return Promise.resolve(s.reply);\n' +
  '  }\n' +
  '  destroy() { this.destroyCount++; if (state().destroyThrows) { throw new Error("private destroy error"); } }\n' +
  '}\n' +
  'export default {\n' +
  '  RequestMethod: { POST: "POST" },\n' +
  '  HttpDataType: { STRING: 0 },\n' +
  '  createHttp: () => new Request()\n' +
  '};');

export function resolve(specifier, context, next) {
  if (specifier === '@ohos.net.http') {
    return { url: HTTP, shortCircuit: true };
  }
  if (specifier === './NetBirdAccountAuthCore' && context.parentURL !== undefined) {
    return { url: new URL('NetBirdAccountAuthCore.ts', new URL('.', context.parentURL)).href,
      shortCircuit: true };
  }
  return next(specifier, context);
}
