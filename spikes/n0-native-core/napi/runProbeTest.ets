/**
 * N0 native core overlay ArkTS test entry.
 *
 * Imports the NAPI overlay from libentry.so and asserts the structured
 * runProbe() fields. Fail-closed: any assertion failure throws.
 *
 * The returned summary is wrapped by the ohosTest test runner
 * (napi/ohosTest/OpenHarmonyTestRunner.ets) into the machine-readable marker
 * `N0_CORE_PROBE_RESULT|verdict=PASS|...`. No private key material is ever
 * printed; the public key is only reported as its length, never its value.
 *
 * This file is staged into a temporary r1-api24-hap snapshot copy by
 * prepare-hap-snapshot.sh; it is not part of the current HAP build.
 */
import n0overlay from 'libentry.so';
import { N0ProbeResult } from 'libentry.so';

const EXPECTED_VERSION_PREFIX = 'n0-native-core/';
const EXPECTED_KEY_LENGTH = 44;

export function runN0ProbeTest(): string {
  const result: N0ProbeResult = n0overlay.runProbe();

  // Assertion order (fail-closed): version -> smoke exists -> smoke.ok ->
  // sub-checks -> key length. The key VALUE is never printed: the key check
  // reports only keyLen (and -1 when the field is not a string).
  if (typeof result.version !== 'string' || !result.version.startsWith(EXPECTED_VERSION_PREFIX)) {
    throw new Error(`N0 runProbe version invalid: ${JSON.stringify(result.version)}`);
  }
  if (result.smoke === undefined || result.smoke === null) {
    throw new Error('N0 runProbe smoke missing');
  }
  if (result.smoke.ok !== true) {
    throw new Error(`N0 runProbe smoke failed: ${JSON.stringify(result.smoke)}`);
  }
  if (result.smoke.x25519Ok !== true || result.smoke.tunnelOk !== true) {
    throw new Error(`N0 runProbe sub-check failed: ${JSON.stringify(result.smoke)}`);
  }
  if (typeof result.key !== 'string' || result.key.length !== EXPECTED_KEY_LENGTH) {
    const keyLen: number = typeof result.key === 'string' ? result.key.length : -1;
    throw new Error(`N0 runProbe key invalid: keyLen=${keyLen}`);
  }

  return `N0 runProbe PASS version=${result.version} keyLen=${result.key.length} ` +
    `tickOp=${result.smoke.tickOp} tickSize=${result.smoke.tickSize}`;
}
