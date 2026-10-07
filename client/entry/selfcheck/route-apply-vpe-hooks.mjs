// Module-resolution seam for run-route-apply-vpe.mts ONLY (host-side fixture).
//
// Same principle as every sibling hook in this directory: NOTHING under test is
// rewritten. Everything platform-ish — @ohos.*, libnetbird_core.so, the sibling
// shims of the byte-identical staged extension — is answered by the EXISTING
// exclusion-generation-hooks.mjs resolver, which this file reuses verbatim by
// calling its exported resolve() (never a copy, so the two can never drift).
//
// This file adds exactly the TWO resolver edges the live-route consumer's VALUE
// imports need, both pointing at byte-identical staged copies of the REAL
// modules written next to the staged extension by the fixture:
//
//   '../routes/NetBirdRouteRequestStore' -> the REAL live-route IPC file adapter
//   './NetBirdRouteRequest'             -> the REAL pure request/result document
//                                          module (this edge is also what the
//                                          staged adapter's own import lands on,
//                                          so the extension's token constants
//                                          and the adapter's parser are ONE
//                                          module instance)
//
// Note the shared resolver keeps its own STUBS entries for these two specifiers
// (added so the OTHER byte-identical staging fixtures, which never drive the
// consumer, still link) — this hook intercepts them first, so only THIS fixture
// gets the real modules.

import { resolve as exclusionResolve } from './exclusion-generation-hooks.mjs';

const STAGED = {
  '../routes/NetBirdRouteRequestStore': 'NetBirdRouteRequestStore.ts',
  './NetBirdRouteRequest': 'NetBirdRouteRequest.ts'
};

export function resolve(specifier, context, next) {
  if (Object.prototype.hasOwnProperty.call(STAGED, specifier) &&
    context.parentURL !== undefined) {
    const dir = new URL('.', context.parentURL);
    return { url: new URL(STAGED[specifier], dir).href, shortCircuit: true };
  }
  return exclusionResolve(specifier, context, next);
}
