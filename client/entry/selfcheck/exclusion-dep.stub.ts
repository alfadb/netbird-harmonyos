// Harness plumbing for run-route-validation.mts ONLY — staged into the temp
// fixture dir as NetBirdEndpointExclusion.dep.ts by route-check-hooks.mjs.
// This is NOT a mirror of any production code: it re-exports the REAL
// byte-identical NetBirdEndpointExclusion.ts fixture and adds one shim const
// for the interface name ('ExcludedRouteEntry') that node's type stripping
// erases, so the ESM linker accepts NetBirdVpnConfig's type-only import.
export * from './NetBirdEndpointExclusion.ts';
export const ExcludedRouteEntry = undefined;
