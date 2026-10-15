// Host seams only; production configuration logic is staged byte-for-byte.
export async function resolve(specifier, context, nextResolve) {
  if (specifier === '@ohos.net.vpnExtension') {
    return { url: 'data:text/javascript,export default {}', shortCircuit: true };
  }
  if (specifier === './NetBirdConnector') {
    return { url: 'data:text/javascript,export const CoreNetworkConfig = {}; export const CoreRouteEntry = {};', shortCircuit: true };
  }
  if (specifier === './NetBirdEndpointExclusion') {
    return { url: new URL('./NetBirdEndpointExclusion.ts', context.parentURL).href, shortCircuit: true };
  }
  return nextResolve(specifier, context);
}
