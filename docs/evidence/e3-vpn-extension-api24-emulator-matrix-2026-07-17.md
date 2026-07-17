# E3 VPN Extension API 24 Emulator Matrix Review

Last reviewed: 2026-07-17

This aggregate review registers the three independently instantiated official HarmonyOS API 24 x86_64 Emulator forms: phone, Tablet, and 2in1. It aggregates reviewed records only; it does not replace any source record, raw artifact, image, instance, or phone E0-E8 gate.

```yaml
evidence_id: EV-E3-API24-EMU-MATRIX-20260717-0001
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E4, E5, E6, E7, E8, R1, R3]
target_tuple:
  distribution: official HarmonyOS 6.1.1(24) API 24 x86_64 phone, Tablet, and 2in1 Emulator images; each target is independently bounded below
  device: independently instantiated phone, Tablet, and 2in1 Emulator targets; no physical device
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: API 24; BMS and Settings registration-layer observations, plus the recorded phone public-API runtime observation only
  channel: N/A; phone used unsigned debug research HAPs; Tablet and 2in1 did not send or install HAPs
code_sha: per referenced record; 2in1 and Tablet code_sha cd6f71c7ecc71e878c5fddea793d65de5b68f31d with no guest source read, build, or execution
upstream_sha: N/A for all referenced E3 records; no Go, NetBird, or other upstream runtime participated
input: reviewed records EV-E3-EMU24-20260717-0003 and 0004, EV-E3-TABLETEMU24-20260717-0001, and EV-E3-2IN1EMU24-20260717-0001 and 0002
expected: register only independently measured official API 24 x86_64 forms and preserve their distinct stopped or runtime boundaries
actual: all three forms lack com.huawei.hmos.vpndialog and VpnServiceExtAbility at the measured BMS/Settings registration layer; phone additionally observed public start pending with zero Extension onCreate, while Tablet and 2in1 stopped at the preplanned registration layer without HAP installation
reviewer: anthropic/claude-opus-4-8 independent API24 Emulator matrix review
reviewed_at: 2026-07-17T22:27:30+08:00
review_record: Verified referenced evidence IDs, authoritative manifests, hashes, target tuples, registration-layer boundaries, phone runtime result, HAP noninstallation for Tablet/2in1, and cleanup records. 0B/0M. The 2in1 source records retain 6 non-blocking minors in their review appendix; the Tablet source record retains its 3 non-blocking minors. This aggregate record is reviewed-pass with verdict blocked; it does not close E3.
verdict: blocked
```

## Reviewed Targets

| Form and reviewed evidence ID | Exact target tuple | Authoritative manifest and reviewed hashes | Result boundary |
| --- | --- | --- | --- |
| phone `EV-E3-EMU24-20260717-0003` | HarmonyOS 6.1.1(24) API 24 `phone_all_x86` research image; `netbird_api24_phone`; x86_64 guest on x86_64 host; explicit Beta HDC `127.0.0.1:10000`; image software 6.1.0.125 | manifest `c330c40e3047857133d623b26709c871a6620da9fac7c177a1c3fc8b6d1a872c`; A HAP `6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c`; B HAP `c1d57d2544a93e4c4f172ee3ecb6ff2659adb7650558957e5b0cfb7aa69ae21e`; transcript `56a13af9e9db5826b15476666b7c6692afe81bfd34c73d56d2708a89cd35e027` | BMS and Settings lack `com.huawei.hmos.vpndialog`/`VpnServiceExtAbility`; ordinary public start remained pending and Extension `onCreate=0`. |
| phone supplemental `EV-E3-EMU24-20260717-0004` | Same independently bounded phone tuple and explicit target `127.0.0.1:10000`; archived A input only; no rebuild | manifest `9e828c63b4dc6f1498d6ca374d6b2e112e071e31269554930129dea8c5250f27`; A HAP `6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c`; transcript `7da7e5b6728eaa7bba5b791532d64ef7939e43970c3539520bb5e571e4a2c894`; full HiLog `4ed0d2f6ff6ca40d56d4fed571f14f846cdb7a8672e436a84bf96f9a9447e0a2` | The 60-second public-API observation remained pending with `onCreate=0`; BMS/Settings registration checks still had no authorization component. |
| Tablet `EV-E3-TABLETEMU24-20260717-0001` | official HarmonyOS 6.1.1(24) API 24 `tablet_x86` Release image; `netbird_api24_tablet` Tablet Emulator; MatePad Pro 13; guest build `emulator 6.1.0.125(SP9DEVC00E16R1P1)`; x86_64 guest on x86_64 host; explicit Beta HDC `127.0.0.1:10002` | manifest `d43bd4f23070eb1ce313ca0fc5fa541735de2d512fab6b48d6a752e6091b781c`; transcript `66f312c845b976bb861fbc88f5ac1929fe3865a7f38ac57486204d8d15ba43c2`; user-100 list `03dafac5361b3f7a376f72846a82692e5f3d7149b167c1e932bac2c8f80e3f2c`; user-0 list `b28bf61bba663c102c32c03a668f9a4fa8f8d8e60c2737ac9628e0c276eb7817` | BMS and Settings registration layer lacks `vpndialog`/`VpnServiceExtAbility`; the preplanned stop condition held, so neither A nor B HAP was sent or installed. |
| 2in1 `EV-E3-2IN1EMU24-20260717-0001` | official HarmonyOS 6.1.1(24) API 24 `pc_all_x86` Release image; `netbird_api24_2in1` 2in1 Emulator; MateBook Pro; guest build `emulator 6.1.0.125(SP12DEVC00E47R1P3)`; x86_64 guest on x86_64 host; explicit Beta HDC `127.0.0.1:10001` | manifest `07dba727f7312de8ba116f5a7976f30bedf4e69d4f576e8e42195a365d37b7f3`; transcript `5e3c8eea9be1a8248153a74a4104fc7480d44f23e6e06a79ebc4908e277729c7`; user-100 list `d39a1ed8414c7f7e0cb120ed245a3ebe257a89d88f2aef63697baa6df3a7c7e7` | User 100 has 49 bundles and zero VPN/vpndialog matches; BMS/Settings registration layer lacks the component. Stop condition held; A/B HAPs were host-hash checked only, never sent or installed. |
| 2in1 supplemental `EV-E3-2IN1EMU24-20260717-0002` | Same preserved official `pc_all_x86`/`netbird_api24_2in1` tuple and explicit target `127.0.0.1:10001`; coldboot/noWindow/KVM recheck | manifest `cf0ec86ace0ab559bad22968cb73c099957f99bf4b1020fa82db2e6b554211a5`; successful transcript `c40b6283c7e88086a9f8b5fe1b9d227cc00614e9146f001b0956e0f46f587815`; user-0 list `91dc0a1b8613e2b60f3b54b860a4ca9366e538c01aaedb9865c942d22a4724dd`; count validation `e2fd48915abf769b3e2d6ba550ae539ebb3e739cc099c95f8810dc39cab3ec2d` | User 0 has 7 bundles and zero VPN/vpndialog matches. The transcript label `0_bundles` means zero VPN bundles (zero `vpn\|vpndialog` matches), not zero total bundles. The earlier connect-key and Offline records are readiness environment traces only. No HAP was sent or installed. |

Each manifest named above is the authoritative full raw-material inventory for its evidence record. The 2in1 pair together covers 49 user-100 bundles, 7 user-0 bundles, and direct `vpndialog` queries in default, user-100, and user-0 scopes. Settings registration searches found no `VpnServiceExtAbility` or VPN extension registration. Cleanup records retain no target Emulator/QEMU/HDC process or listener after each run.

## Matrix Conclusion

All three independently measured official API 24 x86_64 forms are `reviewed-pass/blocked` at the recorded boundary. Their BMS and Settings registration layers lack `com.huawei.hmos.vpndialog` and `VpnServiceExtAbility`. Only phone entered the public API runtime path: it recorded a pending start promise and zero Extension `onCreate`. Tablet and 2in1 intentionally stopped when their registration-layer prerequisite failed; no HAP was installed, so neither has a VPN runtime result.

This result does not extrapolate to arm64, physical devices, other system images, other distributions, or Huawei commercial HarmonyOS. A foldable, triplefold, or widefold configuration that merely reuses `phone_all_x86` is a same-image metadata observation only. No independent foldable, triplefold, or widefold instance was measured; none is listed as a pass or blocked record.

Phone E3 remains blocked, E4-E7 remain dependency blocked and do not start, and E8 remains `CLOSED`. The current action is to wait for an official image that includes the required authorization component and for formal NetBird/Go input changes, then rerun the affected gates against a newly recorded target tuple. A private Go fork, system permission, or system/debug/enterprise bypass is not a recommended substitute for those inputs.
