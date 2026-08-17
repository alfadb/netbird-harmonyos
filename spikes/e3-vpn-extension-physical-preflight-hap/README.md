# E3 Physical VPN Extension Preflight

This is the isolated probe and governed runner for the PLA-AL10 E3-PHYS-PREFLIGHT
on the current frozen device tuple: distribution `HarmonyOS`, model `PLA-AL10`,
full system build `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`, device API `26`, kernel
`aarch64`, app ABI `arm64-v8a`. Settings-page text such as
`7.0.0.100 (SP8C00E32R7P2patch09)` is a manual operator supplement only; the
runner HDC precheck compares the HDC build string to the frozen
`full_system_build` verbatim and never expands or compares `patch09`. It does
not replace or modify the historical API 24 Emulator probe in
`../e3-vpn-extension-hap/` or modify historical raw evidence. The consumed
API 23 initial live record is `EV-E3-PHYS1API23-20260806-0001`; its reviewed
verdict is blocked before continuous capture or installation because the live
build projection drifted from the then-frozen build. Readonly rebind
`EV-E3-PHYS1REBIND7-20260806-0001` is `reviewed-pass/pass` for the three rebind
probes only, not an E3 or campaign pass. `ADJ-20260806-0003` freezes the tuple
above and historically prepared campaign `E3-PHYS-PREFLIGHT-20260806-0002` /
evidence `EV-E3-PHYS1API26-20260806-0001` (never Live, never occupied;
`ADJ-20260807-0001` marks them `superseded-unexecuted`). Host reverify PASS:
public manifest SHA-256
`66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`; FINAL HAP /
signature / profile / member-list hashes unchanged; device install compatibility
is not claimed. `ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` confirms
the HDC build string with one authorized `software.version` probe
(`reviewed-pass/pass`, build-confirm only): exact
`PLA-AL10 7.0.0.100(SP8C00E32R7P2)` matches the frozen binding and removes the
synthetic-build residual. API `26` / `aarch64` / `arm64-v8a` remain prior rebind
measurements and are not inferred from the build string. `ADJ-20260807-0001` renumbered the then-unexecuted API26 campaign to
`E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`; that pair
later went Live and is now `consumed-blocked` (operator-aborted procedural seal;
scenario-5 Settings misconfirmation and direct window close; recovery cleanup
`verified_absent`; independent seal review 0 B/M; no partial scenario-5 replay).
`ADJ-20260807-0002` authorized a Chinese full 1-7 rerun as protocol usability
correction (not device-behavior retry). That campaign
`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` went Live
and is now `consumed-blocked` (`reviewed-pass/blocked`; S1/S4 pass,
S2/S3/S5/S6/S7 blocked; cleanup verified-clean; dual independent review 0 B/5 M;
opus timeout attempt-not-counted; prior 0001 retained). `ADJ-20260807-0003`
(user direct decision) then approved a host process terminal probe and the
Settings app-info force-stop revoke path: scenario 3/7 prefer the callback
destroy terminal + post-destroy fd snapshot, otherwise fall back to a strict
process-boundary route (unique stop + onDestroy + destroy-begin/pre snapshot +
consecutive absent host `PidOf`/`BundleDump` probes, >=2 probes >=3s apart,
bundle present for scenario 3; `FD_STILL_OPEN` is never overridden; scenario 3
strict pass additionally needs a scenario 5 fresh create as clean reactivation
proof or overall stays blocked); scenario 5 revokes via manual Settings > app
info > A > force stop with a separate screenshot and
`SETTINGS-APP-INFO-FORCE-STOP-CAPTURED` confirmation, while the Settings VPN
page is observation-only and the runner never issues HDC force-stop (HDC
force-stop is explicitly cleanup-only, restricted to exception/final cleanup).
(**历史**：`ADJ-20260808-0002`/`0003` 强可靠模式前瞻取代该人工确认——S5 改为机器化
Settings 应用信息强制停止，`SETTINGS-APP-INFO-FORCE-STOP-CAPTURED` 与 Settings>VPN
observation 均不再询问，仅 historical。)
`ADJ-20260808-0001` (prospective minimal fix for the sealed blocked record)
then registered the process-boundary semantic change: `PidOf` targets the exact
`<bundle>:vpn` Extension ability process instead of the bundle UI process (the
UI process keeps running under a normal Stop, so bundle-level pidof is
physically unsatisfiable), `BundleDump` still proves the bundle stays
installed, every probe and the S3/S5/S7 records carry `process_target`, and the
freeze adds the required `process_probe_target` field; the same execution also
adds UI last-known request id (no first-stop skip, no manual second Start),
S4 deny pre-capture before the Deny click, S5 probe scheduling that actually
reaches >=3.0s (exact DateTimeOffset + margin, keeps probing on insufficient
spacing), the pollable `operator-wait-state.json` (EvidenceRoot, manifest
sealed, no target/UDID/HAP-path/endpoint/secret), and the tri-state
`scenario_aggregation.s3_clean_reactivation_proof`. That semantic change
applies only to the next new campaign/evidence and never rewrites any sealed or
historical record.
Registered HAP artifacts remain compile API `24` with target/compatible API
`23`; device freeze API is `26` and compatibility is measurement-only. API26
0001 used runner SHA-256
`19fc1a76e49b9dca66a8a0352cc6bc8291f2888e66b3ad72cdc8a91ed97312e7`; API26 0002
used runner SHA-256
`2fb2d3e99585a53adec82ea3b51ae2ea29c8f021d46e24b0828faa5415d38194` and code
`e8eb1b67a48603c55d3f55d2be686bae0dbd15e1` (freeze
`d6334c2d8d0d1bf11a2a9e26f65039ee0a1a98e377fbf644cef557ff02c55a1a`, now
`CONSUMED-BLOCKED`); the runner has since changed under `ADJ-20260807-0003`, so
a new live freeze must bind a fresh commit and runner SHA-256. As of the
2026-08-08 host-remediation snapshot, `plan_status` was
`blocked-awaiting-device-authorization` and the Windows signing/build host had
rebuilt the runner, freeze, and selftest snapshots and registered them as
[`EV-E3-PHYS1HOST-20260808-0001`](../../docs/evidence/e3-physical-preflight-host-remediation-2026-08-08.md)
(host selftest `HDC_PROCESSES=0`, independent review 0 B/0 M; the old
candidate pair `E3-PHYS-PREFLIGHT-20260808-0001` /
`EV-E3-PHYS1API26-20260808-0001` prepared with freeze `plan_status: blocked`;
DryRun `is_evidence: false`/HDC0/integrity empty; old 20260807 candidate
`INVALID-TIMELINE` unusable). That old candidate pair is now consumed (sealed
blocked external evidence, see below) and must never be reused. **Superseded
(2026-08-10 · AUTH-E3-PHYS1API26-20260810-0002)**: replaced by
[`AUTH-E3-PHYS1API26-20260813-0001`](../../docs/evidence/e3-physical-preflight-authorization-2026-08-13-0001.md)
(execution host migrated from the Windows signing/build host to the current
Linux Pod; 0002's historical record is kept unchanged). Under 0002 the user
explicitly
authorized a NEW full-whitelist campaign
([`e3-physical-preflight-authorization-2026-08-10-0002.md`](../../docs/evidence/e3-physical-preflight-authorization-2026-08-10-0002.md);
supersedes the consumed
`AUTH-E3-PHYS1API26-20260810-0001`) with the new candidate pair
`E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`,
`attempt=initial`/retry N/A, any gate failure stops with no retry and no ID
switch, two new-pair consumption audits (audit-1 before any
`-TargetBindingConfirm`, audit-2 after the final ready freeze; both hash
recorded), and a single memory-only `hdc list targets` host-prep exception
(exactly one token, process-scope `PHYS_1_TARGET`, no output/persistence, runner
HDC whitelist NOT expanded). The old pair `E3-PHYS-PREFLIGHT-20260808-0001` /
`EV-E3-PHYS1API26-20260808-0001` is consumed by the external sealed blocked
evidence `EV-E3-PHYS1API26-20260808-0001` (record_status=collected /
overall=blocked / verdict=blocked, execution_mode=live, is_evidence=true;
campaign-seal SHA-256
`ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f` sealed_at
2026-08-08T09:53:23+08:00; hash-manifest SHA-256
`36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`;
legacy-pair-consumption-audit `id-consumption-audit-1.txt` SHA-256
`b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`,
2026-08-10T10:43:09+08:00) and must never be reused. No auto retry, no
ID switch; any later attempt requires new governance. This registration bans
HDC (except the single memory-only host-prep `hdc list targets`); E8 remains
CLOSED. Prior campaign/evidence IDs must not be reused. The checked-in freeze example remains intentionally `blocked`.

## Boundary

The project has two ordinary Stage application products with independent
identities:

- logical `vpnA`: Hvigor product `default`, bundle
  `cn.alfadb.netbird.e3physvpna`;
- `vpnB`: Hvigor product `vpnB`, bundle `cn.alfadb.netbird.e3physvpnb`;
- one exported ordinary `EntryAbility` whose only commands are Start and Stop;
- one non-exported `E3PhysicalVpnExtensionAbility` of manifest type `vpn`;
- only `ohos.permission.INTERNET`.

`AppScope/app.json5` uses the default product's A identity rather than a third,
non-buildable generic identity. The `vpnB` product override remains explicit in
`build-profile.json5`. Packaged `module.json` and `pack.info` are audited for
the selected product identity and API contract.

The only native payload is the necessary pure C read-only fd-state probe
`libs/arm64-v8a/libfdprobe.so`. Its only JS operation is
`status(fd): { open, errno }`, implemented with `fcntl(fd, F_GETFD)` and
`errno`. It does not close, duplicate, read, write, create, or transfer an fd;
it has no socket, network, thread, packet, Go, NetBird, or WireGuard path. The
HAP must contain exactly that one arm64-v8a shared library and no other native
payload.

There is no `MANAGE_VPN`, system, debug, enterprise, or automatic-authorization
permission or path. There is no authorization observer, VPN ID, socket
protection, external endpoint, packet pump, or background service. This
project never signs, generates keys, logs in, invokes HDC, installs, or contacts
a device or network.

## FD Ownership And Lifecycle

OpenHarmony-6.1-Release establishes the ownership boundary used here:
`NetworkVpnClient::DestroyVpn(bool)` calls
`vpnInterface_.CloseVpnInterfaceFd()` before it calls the service proxy, and
`VpnInterface::CloseVpnInterfaceFd()` closes its internal TUN fd and resets it
to zero. Therefore the platform `VpnConnection.destroy()` path is the sole
owner of close. Neither ArkTS nor `libfdprobe.so` calls close, dup, read, or
write on the returned fd.

After `VpnConnection.create(config)` resolves, the Extension stores `tunFd` and
immediately emits `VPN_FD_SNAPSHOT|phase=post-create`. Create is accepted only
when the returned value is a nonnegative integer and the native probe confirms
`open=true`. A resolved but invalid or not-open fd is not classified as create
rejection: it emits `CREATE_INVALID_DESTROY_REQUIRED` and invokes the same
connection's single-flight `destroyOnce()` exactly once for self-cleanup. A
rejected or synchronously failed create, including an A/B conflict rejection,
never calls destroy.

`destroyOnce()` is idempotent and waits for a pending create promise. Before a
required destroy it emits `phase=pre-destroy`. After both destroy resolution
and rejection it emits `phase=post-destroy-resolved` or
`phase=post-destroy-rejected`, including explicit `open=true|false` and one of
the fd decision markers. It never actively closes the fd.

The official HarmonyOS 6.1 VPN Extension pattern calls the asynchronous
`destroy()` promise from the synchronous `onDestroy(): void` callback. This
project keeps that fire-and-forget pattern and records the later terminal
markers; it does not busy-wait or block the ArkTS event loop. `onDestroy`
returning is not a cleanup result. Likewise, `STOP_PROMISE_RESOLVED` only means
the UI stop request settled. Cleanup may be judged only from a matching
`VPN_DESTROY_RESOLVED` or `VPN_DESTROY_REJECTED` terminal marker together with
the corresponding post-destroy fd snapshot.

The fd probe observes only whether that numeric descriptor is open at the
snapshot instant. It cannot prove fd identity or prevent descriptor-number
reuse. A missing terminal marker or `FD_STATE_UNCONFIRMED` remains
inconclusive; `FD_STILL_OPEN` is a cleanup failure signal requiring review.

## Operator Sequencing

The UI disables both commands while a Start or Stop promise is pending and
disables Start while an active request id exists. Start rejection clears only
its matching id. The UI keeps a last-known request id (`lastRequestId`): a new
Start overwrites it, the bounded pending release never clears it, and Stop with
an empty active id falls back to it with an explicit `basis=last-known-request`
marker so the first S3 Stop is never skipped and no manual second Start is
needed (which would otherwise mismap request ids). Stop without any active or
last-known id emits `UI_STOP_SKIPPED` and does not invent an id. A successful
Stop, or a terminal rejection (BusinessFailure code `16000001`), clears the
matching ids; that does not itself permit the next scenario to begin.

For every scenario, wait for the matching Extension create result before
issuing Stop. After Stop settles, wait for the matching Extension destroy
terminal marker and post-destroy fd snapshot before any next Start, bundle
switch, or scenario transition. `START_PROMISE_RESOLVED` and
`STOP_PROMISE_RESOLVED` are transport/lifecycle request observations, not VPN
create or cleanup proof.

When Settings revokes the VPN or an A/B replacement tears down the active
extension, first wait for the matching `VPN_DESTROY_RESOLVED` or
`VPN_DESTROY_REJECTED` terminal marker and its post-destroy fd snapshot. If
the page remains locked after that accounting, press Stop. A subsequent Start
is allowed only after matching `STOP_PROMISE_RESOLVED` or the precise
`STOP_SESSION_RELEASED|...|reason=ability-not-found` reconciliation marker,
which is emitted only when Stop rejects with BusinessFailure code `16000001`
(`specified ability does not exist`). Every other Stop rejection remains
blocked and retains the active request id. `STOP_PROMISE_RESOLVED` is never
cleanup evidence: even when it releases the UI lock, the matching Extension
terminal marker and post-destroy fd snapshot are still required.

### Live 人工操作（中文，`ADJ-20260808-0002` 强可靠模式）

A/B 指两个测试 App。**当前规则**（`mechanical-action-only-machine-verified-v1`）：操作员每步只看到“现在只做：X。完成后按回车。”；按回车仅表示机械完成，**不**解析 y/n/token/READY/ACK。机器负责 layout/事件/进程判定。旧 READY/ACK nonce 与 operator 三态确认仅为 **historical**。

1. 场景1：runner 自动查询/暂存/安装 A 与 B（无操作员语义门）。
2. 场景2：按提示点 A Start → 等机器确认授权页 → 按提示点 Allow → 等机器判定。
3. 场景3：按提示点已激活 A 的 Stop → 等机器判定（额外 Start / `UI_STOP_SKIPPED` / 错误 requestId → invalid）。
4. 场景4：按提示点 B Start → 等机器确认授权页 → 按提示点 Deny → 等完整窗口机器判定（无人工 DENY-SCREEN）。
5. 场景5：按原子步骤依次：A Start（可选 Allow）→ 打开 A 应用信息页 → `点击强行停止，并完成随后出现的确认（如有）`；每步回车后机器判定（Settings>VPN 页不再询问，`not-required`）。打开应用信息页后的 capture 是严格 `Setting.AppDetail` 结构门；点击后的 capture 仅保留为 observation-only 证据，页面形状或一般 capture 退化不独立判废；连续 HiLog stream 已 degraded 仍按全局安全规则 blocked。runner 不自动点击、不猜确认框结构；最终只由既有连续 `<bundle>:vpn` absent + bundle present 后置门确认撤销效果。
6. 场景6：按提示点 A Start（可选 step2 reauthorization Allow），再点 B Start（step3）；B Start 后机器以 entry/authorization 双档案 checkpoint 分类。entry 直接等 B terminal；authorization 才提示 step4 Allow，step4 precondition 只绑定已机器验证的 authorization layout/request，dismissed 后再等同一 B request terminal。B Start 前 A exact-process 是 pre-gate；accepted/rejected terminal 后都只观察一次 terminal checkpoint 并写入 record，但只对 rejected gate。terminal 后统一完成 context、unexpected accepted 扫描、event contract 与 verified A/B request accepted-marker 计数；foreign/missing accepted 优先 invalid。任何 B accepted/双 accepted 先判功能 fail，再判 rejected checkpoint、nonfrozen 与窗口退化 blocked；冻结拒绝且 A verified 才 pass。已收集的未 dismiss mismatch 与已收集但 layout unverifiable 均 blocked 但 reason 不同；capture 未收集沿既有 decisive gate invalid/infra blocked；无 terminal 保持 runner blocked。step4 prompt/postcondition 间 stray action 归因 step4 invalid，其他额外 UI action仍 invalid。
7. 场景7：按提示点 S6 绑定的 active A 的 Stop；**不要**手工强停或卸载；无 FINAL-CLEANUP 确认；runner 负责清理。

## API Mapping

The Stable Command Line Tools 6.1.1.290 SDK declares HarmonyOS 6.1.1/API 24.
HAP artifact manifests still compile against `6.1.1(24)` and declare target and
compatible SDK `6.1.0(23)`; those HAP compile/target/compatible values are not
the device freeze API. Do not rewrite the HAP contract to device API `26`.
Stable Hvigor 6.24.3 requires one product literally named `default`, so logical
`vpnA` uses that name.

The public API used here is available from API 11:

- `startVpnExtensionAbility(want)` and `stopVpnExtensionAbility(want)` are
  called only by the UI;
- `createVpnConnection(this.context)` is called exactly once by the Extension;
- `VpnConnection.create(config)` receives only `192.0.2.1/32`;
- the same connection's no-argument `destroy()` is the only close owner;
- the pure C probe only observes fd state and never assumes ownership.

## Build And Audit

Run independent clean unsigned builds with the Stable CLI from this directory:

```bash
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean \
  --mode module -p product=vpnB -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap \
  --mode module -p product=vpnB -p module=entry@default \
  -p buildMode=debug --no-daemon
```

Expected outputs:

```text
entry/build/default/outputs/default/entry-default-unsigned.hap
entry/build/vpnB/outputs/default/entry-default-unsigned.hap
```

Run the local audit, which repeats clean builds for both products and checks
source boundaries, markers, identities, packaged metadata, the sole native
member, AArch64 ELF identity, allowed dependencies, and artifact hashes:

```bash
bash ./audit-physical-preflight.sh
```

The Linux audit covers only unsigned local-preparation artifacts. It does not
approve a signing profile, produce the final signed A/B campaign HAPs, or make
them installable campaign inputs. The signing/profile and final-HAP freeze gates
remain external to this audit. The audit does not invoke HDC, sign, install,
start an Emulator, contact a physical device, log in, download dependencies, or
write evidence/governance files.

## Windows Campaign Runner

`e3-phys-preflight-campaign.ps1` is the only runner for the single governed
`E3-PHYS-PREFLIGHT` campaign. It requires PowerShell 7 or later. Live mode is
refused unless the git worktree is clean, an out-of-repository freeze manifest
has `plan_status: ready`, and all campaign, attempt, target tuple, signing,
final HAP, source, SDK, HDC, runner, collection, cleanup, and independent-review
inputs pass before-install validation. The real HDC target is accepted only
from the process environment variable `PHYS_1_TARGET`; projected transcripts
use only `PHYS-1` and redacted values. The frozen HDC version output and
executable SHA-256 must match `HdcPath`. Every bounded HDC call has a timeout.

The checked-in `e3-phys-preflight-freeze.example.json` is intentionally blocked
and contains placeholders rather than campaign hashes, local paths, signing
material, endpoint data, or current secret values. Its example target tuple is
the current freeze (`HarmonyOS` / `PLA-AL10` /
`PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / API `26` / `aarch64` / `arm64-v8a`);
campaign and evidence IDs remain generic blocked placeholders
(`E3-PHYS-PREFLIGHT-<CAMPAIGN-CODE>`,
`EV-E3-<TARGET-CODE>-<YYYYMMDD>-<SEQUENCE>`). Its required field groups
are campaign identity/status/retry, the exact target tuple and 60-second window,
settings re-allow expected path defaulting to `direct-system-activation` with
required `settings_reallow_path_policy: observation-only`, the
`ADJ-20260807-0003` decision fields `settings_revoke_mechanism`
(`settings-app-info-force-stop`), `settings_vpn_page_policy`
(`observation-only`), `destroy_terminal_policy`
(`callback-or-strict-process-boundary`), `process_absent_required_count` (`2`)
and `process_absent_probe_spacing_seconds` (`3`), the `ADJ-20260808-0001`
decision field `process_probe_target` (`<bundle>:vpn`), ordinary-development
signing, final A/B artifact hashes, frozen source archive/manifest, SDK input
map, HDC version/hash, runner/code hashes, freeze time, cleanup/collection/review
Boolean gates, and distinct operator and reviewer roles. The path policy and the
six decision fields are part of the freeze contract hash; old freezes without
these fields are historical only and are rejected for every mode (DryRun
included), never usable for a new live.
Scenario 5 records `settings_reallow_path` expected/actual/match/observation and
never blocks solely because actual path differs from the predicted path; the
revoke mechanism is now `settings-app-info-force-stop`: a fresh A
start/create-accepted/post-create-open first (with optional reauthorization
Allow, like S2), then a machine-verified Settings > app info > A gate followed
by the operator action `点击强行停止，并完成随后出现的确认（如有）`. The pre-action
app-info capture is the strict deterministic layout gate; the post-action
capture is observation-only and cannot independently decide the campaign, while a degraded continuous HiLog stream remains blocked by the global safety rule. Pass requires the
machine settings-app-info layout gate, fresh create/open, bundle still present, and
consecutive absent `<bundle>:vpn` Extension-process probes
(>=2 probes >=3s apart, `process_target` recorded; scheduling actually reaches
the frozen 3.0s via exact DateTimeOffset round-trip plus a small margin, and
insufficient spacing keeps probing instead of finishing early); no `UI_STOP` is
required or expected on this path. HDC never issues
force-stop for the revoke.
`signing.device_in_profile` is JSON Boolean `true` in the example to show the
required type; the adjacent placeholder basis explicitly means it is not a
checked-in device-profile claim. Device HiLog zone mapping (`CST=>+08:00`) and
the 3-second device-clock skew tolerance are runner constants, not
freeze-manifest fields; they appear only on the emitted record's `clock_source`.

After HDC/device precheck, Live starts one continuous campaign HiLog process and
records an initial complete-line/byte anchor. Scenarios never restart that
stream. Each scenario records a fresh byte anchor before the mechanical action
prompt, filters only new complete lines by parsed `device_observed_at` from the
action prompt through the actual observation end (pre-enter
`relative_to_prompt` events are stamped against the prompt time so a slow
operator never pushes device events past the enter), retains `host_observed_at`,
and requires measured healthy coverage through at least 60 seconds after the
action. The operator only sees “现在只做：X。完成后按回车。” and Enter is a
mechanical completion — there is no READY/ACK/token/y-n semantic gate (old
READY/ACK nonce is historical only). Install
is three-state: only explicit rejection
evidence (install failed / error code / signature or profile reject, and similar)
is `FUNCTIONAL_FAIL`; clear success string plus BundleDump presence is pass;
success with unrelated warnings, dump unavailable/permission, or uncertain output
is non-infrastructure blocked (no fail, no retry authorization). Exit 0 alone
never marks Installed. Staging sets `StagingMayExist` before `MkdirStaging` so
any later failure still runs fixed staging cleanup and StagingProbe in finally;
removal clears flags only after a fixed-path probe shows clear path absence
(`no such file` / `not found` / `path does not exist`). `cannot access` and
`permission denied` are residual/unknown, not absence. Continuous capture
degradation entries carry `category` and `infrastructure_reason`. Only real
capture process exit, stderr growth, start failure, or HDC timeout/offline is
`hdc-usb-interruption`; time-parse, format, missing screenshot/layout, and
permission/unsupported fault artifacts are non-infrastructure blocked. Generic
`capture_degraded` never maps to USB. Targeted fault artifact failure records
FaultArtifacts/CaptureDegraded and can block scenario 7 only; it must not mark
continuous `Capture.Degraded` or shorten the 60-second window. Record
`target_tuple.distribution` is taken from the freeze, not hard-coded. Scenario 2
captures the visible system authorization UI (screenshot + layout) with a
machine deterministic layout gate before Allow (no operator visibility
confirmation — that manual confirmation is historical only); capture failure at
a decisive gate blocks scenario 2 and keeps the artifact reference.

Under `ADJ-20260807-0003`, scenarios 3/5/7 run host process terminal probes
using only the allowlisted `PidOf` and `BundleDump` (bundle presence)
observations. Under `ADJ-20260808-0001`, `PidOf` targets the exact
`<bundle>:vpn` Extension ability process (never the bundle UI process, which
keeps running under a normal Stop with the UI visible and makes bundle-level
pidof physically unsatisfiable), and `BundleDump` continues to prove the
bundle/main App stays installed; every probe and the S3/S5/S7 scenario records
carry `process_target`. `Get-VpnFinalState` prefers the callback destroy
terminal + post-destroy fd snapshot (`terminal_mode=callback-post-fd`);
`FD_STILL_OPEN` is a hard fail that never falls back. Otherwise the strict
process-boundary route requires a unique current-window stop for the same
bundle/request, `UI_STOP` or `STOP_PROMISE_RESOLVED`, `VPN_ONDESTROY`,
`VPN_DESTROY_BEGIN` or a pre-destroy fd snapshot, and at least
`process_absent_required_count` consecutive absent probes spaced
`process_absent_probe_spacing_seconds` seconds apart; scenario 3 additionally requires the bundle present
during the probes, and `VPN_DESTROY_ISSUED` never counts. Every probe (time,
status, consecutive-absent count, bundle present) is recorded into the scenario
entry and transcript; present/unknown/error resets the counter, unknown/error
aborts the series, and finally/uninstall-absent never backfills. Scenario 3
strict-fallback pass also requires a scenario 5 same-bundle fresh
`CREATE_ACCEPTED` + post-create open as clean reactivation proof or the overall
aggregation stays blocked. Scenario 5 revokes via Settings app-info force-stop (fresh A
create first, a strict machine settings-app-info gate before the action, then
`点击强行停止，并完成随后出现的确认（如有）`, consecutive absent `<bundle>:vpn`
probes, bundle still present). The post-force capture is observation-only and
may show a different page structure or ordinary capture degradation without independently changing the verdict; a degraded continuous HiLog stream remains blocked by the global safety rule; no `UI_STOP`
is expected and no manual
`SETTINGS-APP-INFO-FORCE-STOP-CAPTURED` confirmation is asked (historical under
`ADJ-20260808-0002`/`0003`). Scenario 7 runs pre-uninstall probes and allows
the existing uninstall cleanup only after the terminal assessment completes.
`ADJ-20260807-0003` adds no new exit criteria: it only corrects the S3/S5/S7
terminal-state priority (callback terminal + post-destroy fd snapshot first,
`FD_STILL_OPEN` hard fail, then the strict process-boundary fallback) and the
S5 revoke mechanism. Under `ADJ-20260808-0002`/`0003` the strong-reliable mode
runs every scenario machine-verified; an explicit functional fail (extension
create rejected/invalid fd after `VPN_ONCREATE`, deny-then-create, replacement
destroy fail) always outranks capture/window degradation and is never
downgraded to blocked, while missing evidence under degradation stays blocked
and is never promoted to fail. S6 is fully machine: unique A `CREATE_ACCEPTED`
plus unique B `CREATE_REJECTED` with the frozen conflict code `2203002`; a pure
authorization-layer A outcome (`START_PROMISE_REJECTED` or a reject with no
`VPN_ONCREATE`) is blocked `authorization-outcome-unclassified` (not fail, not
invalid), a non-frozen B rejection code is blocked
`B-conflict-code-not-frozen:<code>`, and S7 stays
`not-run-after-platform-blocked` after a platform block. **Historical** (not the
current rule): the operator dual-active three-state confirmation
(`NO-DUAL-ACTIVE-CAPTURED` / `DUAL-ACTIVE-CAPTURED`) and the
`no_dual_active_confirmed` / `dual_active_confirmed` / `operator_state` record
fields were deprecated and removed under `ADJ-20260808-0002`/`0003`. The
Settings>VPN page capture in scenario 5 is `not-required` under
`ADJ-20260808-0003` (never asked, never invalid/block/pass input); its failure
still writes an independent `observation_only_degraded` diagnostic, but never
calls `Add-CaptureDegradation`, never enters the global `capture_degraded`
list, and never blocks scenario 5 or the final overall; the pre-action
`scenario-5-app-info` layout gate remains decisive, while the post-force capture is observation-only. A degraded continuous HiLog stream remains blocked by the global safety rule. The
freeze decision field is `process_absent_probe_spacing_seconds`; the legacy
`spacing` name is rejected as unknown/missing for every mode and never reused
compatibly.

`EvidenceRoot` contains only the redacted structured projection, record,
operator attestation, collection manifest, lock, and seal. Unfiltered HiLog,
screenshots, layouts, and targeted A/B fault outputs go to an independent
out-of-repository `RawRoot` sibling (by default `<EvidenceRoot>.raw`), never to
`EvidenceRoot` or this repository. There is no raw transcript:
`projection/transcript.redacted.jsonl` is the only transcript. Recursive leaf
redaction occurs before JSON serialization, and every transcript `entry_hash`
binds the stored `payload_canonical` string (`SHA256(payload_canonical)`).
Integrity verification parses each JSONL line with `System.Text.Json.JsonDocument`,
compares `payload` raw text to `payload_canonical` verbatim (no
`ConvertFrom-Json` object roundtrip, which would drift ISO dates / single-element
arrays), checks `previous_hash` against the prior recalculated entry hash, and
verifies index order. Live runner records may be `collected`,
`blocked`, or `invalidated`; reviewed states remain exclusive to the independent
review step.

Run the host-only fixture suite. It parses both scripts and exercises the
sanitized production-derived S5 app-info fixture (provenance: `docs/evidence/e3-physical-preflight-production-layout-2026-08-15.md`), a blocked
plan dry-run plus injected seven-scenario `LiveSimulation`, repository gates,
retry authorization, continuous capture anchors, partial lines, capture death,
cleanup verification, redaction/JSON shape, fault hashes, lock, junction,
timing, argv substitution, and integrity cases. Its HDC path is an executable
sentinel, and the suite fails if that process starts:

```powershell
pwsh -NoProfile -File .\tests\e3-phys-preflight-runner-selftest.ps1
```

Run a governed host-only dry-run after preparing an out-of-repository freeze
manifest and final input files:

```powershell
pwsh -NoProfile -File .\e3-phys-preflight-campaign.ps1 `
  -FreezeManifest C:\outside-repo\freeze.json `
  -EvidenceRoot C:\outside-repo\evidence-dry-run `
  -RawRoot C:\outside-repo\evidence-dry-run.raw `
  -HapA C:\outside-repo\final-a.hap `
  -HapB C:\outside-repo\final-b.hap `
  -HdcPath C:\tools\hdc.exe -DryRun
```

`-DryRun` accepts `plan_status: blocked` or `ready` but always emits an explicit
non-evidence blocked record. `-DryRun`, `-SelfTest`, and injected
`-LiveSimulation` mechanically keep the HDC process count at zero.
`LiveSimulation` always records `is_evidence: false` and `record_status: blocked`;
`verdict`/`overall` stay `blocked` unless a scenario measured an explicit fail
(e.g. post-destroy `FD_STILL_OPEN`), which must survive capture degradation as
`fail` and is never downgraded to blocked.
A retry prior must instead be a frozen matching Live blocked evidence record.

### TargetBindingConfirm (host-governed machine fresh confirmation)

`ADJ-20260810-0001` adds the mutually exclusive `-TargetBindingConfirm` mode,
which resolves the ready-before-fresh catch-22: a real-device fresh
confirmation must run before a `ready` freeze exists, yet Live only accepts
`ready`. The mode is a host-governed single-purpose probe, **not** a campaign:

- Requires `-ConfirmationRecord <out-of-repo-path>` (only in this mode); the
  path must be outside the git repository and neither the record nor its
  `.sha256` companion may already exist (single-use immutable double file). It
  is mutually exclusive with `-DryRun`, `-LiveSimulation`, and `-SelfTest`,
  and it **explicitly rejects** `-EvidenceRoot`/`-RawRoot` (it never
  initializes campaign roots) rather than silently ignoring them. Mode
  exclusivity is enforced before the `-SelfTest` early exit, so invalid switch
  combinations are rejected even with `-SelfTest` present.
- Fixes, under the authorization `AUTH-E3-PHYS1API26-20260810-0002`
  (see
  [`e3-physical-preflight-authorization-2026-08-10-0002.md`](../../docs/evidence/e3-physical-preflight-authorization-2026-08-10-0002.md);
  **superseded by `AUTH-E3-PHYS1API26-20260813-0001`** - execution host
  migrated to the current Linux Pod, 0002's historical record kept unchanged;
  supersedes the consumed `AUTH-E3-PHYS1API26-20260810-0001` whose old
  candidate pair `E3-PHYS-PREFLIGHT-20260808-0001` /
  `EV-E3-PHYS1API26-20260808-0001` is sealed-blocked consumed and must never
  be reused), the candidate pair `E3-PHYS-PREFLIGHT-20260810-0001` /
  `EV-E3-PHYS1API26-20260810-0001` and `attempt=initial` with
  `retry.basis`/`infrastructure_reason=N/A`: the generic infrastructure retry
  branch never applies to this path, any gate failure stops with no retry and
  no ID switch, and any later attempt requires new governance.
- Accepts a freeze with `plan_status: blocked` or `ready` and still runs the
  full `Assert-FreezeManifest` structural gate, the clean-repository gate,
  `code_sha`/runner hash, HDC version + executable SHA-256, external
  HAP/source/SDK input hashes, and the controlled `PHYS_1_TARGET` single-token
  check. It does **not** require campaign readiness, never initializes
  `EvidenceRoot`/`RawRoot`, never sets `is_evidence`, and never consumes
  campaign/evidence IDs.
- Executes real HDC exactly three times, strictly via the existing allowlisted
  argv (`Version`, `TupleModel`, `TupleBuild`); the existing redacted command
  projection is reused and the real target never enters any projection or
  record. It never enters continuous capture, install, start, or final
  campaign cleanup queries (drift therefore needs no device-side cleanup
  here; the Live precheck that follows is a normal campaign flow with targeted
  cleanup verification). Model/build must match the freeze verbatim.
- Writes the confirmation record as a **double-file completion pair**: JSON
  tmp + `.sha256` tmp are written first (no-clobber), the hash is recomputed
  over the tmp JSON, the JSON is atomic-moved into place, and the companion is
  atomic-moved LAST as the completion marker. A consumer only accepts the
  record when both files exist and the companion matches the record bytes; a
  companion failure may leave an orphan JSON that is never consumed, never
  overwritten, and the run returns `blocked` (exit 2). The JSON carries
  `schema_version=1`, `record_kind=target-binding-confirmation`,
  `is_evidence=false`, `authorization_id`, `exception`, the exact candidate
  pair, `attempt=initial`/retry N/A, `code_sha`, runner/freeze/HDC hashes,
  `confirmation_contract_sha256` (the **stable** two-phase contract, see
  below; never the full freeze contract), alias `PHYS-1`, `target_redacted=true`,
  expected/observed model+build, started/ended,
  `command_attempted`/`command_completed` (both 3 on pass; `command_count` is
  a compatibility alias the consumer requires to equal `command_completed`),
  and verdict `pass|blocked` plus reason. Every
  device-observed value (version/model/build) and the reason go through
  `Protect-SensitiveText` before they enter the record; no target/serial/UDID/
  secret ever appears. The pass exit is mechanical: the verdict is only
  produced when exactly three HDC processes were started
  (`HdcProcessStartCount=3`) and attempted=completed=3, asserted BEFORE the
  record is written (a pass double-file pair is never generated and then
  downgraded); any mismatch stays blocked, and a blocked record may carry any
  attempted/completed <= 3 (partial probe progress). Pre-record gate failures (record/companion already
  exist, in-repo path, reparse ancestor) throw and exit 1 with no record
  written; probe/tuple or record-write failures write a best-effort blocked
  record + companion and exit 2. Returned SHA-256 is recomputed from the final
  moved file (return/disk same source).

Run it from the spike directory (the runner resolves the repository root from
`$PSScriptRoot`, so the repo root is independent of the current directory; the
cwd only matters for the example's relative parameters such as
`\e3-phys-preflight-campaign.ps1`) with a blocked confirmation freeze that
carries `machine_fresh_confirmation.pending`:

```powershell
pwsh -NoProfile -File .\e3-phys-preflight-campaign.ps1 `
  -FreezeManifest C:\outside-repo\freeze-blocked-confirm.json `
  -ConfirmationRecord C:\outside-repo\target-binding-confirmation-20260810-0001.json `
  -HapA C:\outside-repo\final-a.hap `
  -HapB C:\outside-repo\final-b.hap `
  -HdcPath C:\tools\hdc.exe -TargetBindingConfirm
```

Pass (`exit 0`) prints `RUNNER_RESULT=pass MODE=target-binding-confirm
RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false
COMMAND_ATTEMPTED=3 COMMAND_COMPLETED=3 RECORD=<path> RECORD_SHA256=<sha>`;
any failure prints `RUNNER_RESULT=blocked` and exits 2 (probe/tuple/write
failure) or 1 (pre-record gate failure with no record). Confirm mode has no
EvidenceRoot, so no transcript projection exists in this mode (the no-op
`target-binding-confirm-record` transcript entry was removed); the campaign
path projects `machine_fresh_confirmation` and the symmetric
`independent-review-record` projection (status/authorization_id/reviewer_role/
record_sha256/record_path_sha256, never the real path) into the preflight
transcript and the sealed complete record, anchored to the stable confirmation
contract. The sealed complete record also projects the standard final
`freeze_contract_sha256` (full contract of the final ready freeze) alongside
the stable `confirmation_contract_sha256`, so either binding can be verified
without re-derivation.

The `ready` freeze that follows must bind the record via
`machine_fresh_confirmation` (`status=pass`, matching `authorization_id`,
`record_path`, `record_sha256`): Live and a DryRun of a `ready` freeze fully
validate the out-of-repo record + matching companion and every content field
(schema/kind/is_evidence=false/exception/exact candidate pair/attempt
initial/retry N/A/device_alias/target_redacted/verdict pass/reason
N/A/code/runner/HDC SHA + version/confirmation_contract_sha256/expected+observed
model+build/command_attempted=3/command_completed=3/command_count alias
match/started<=ended<=`preflight_inputs_frozen_at`). The consumer also
rejects any unknown top-level field (exact schema), so a target/serial/
secret canary smuggled into the record makes it un-consumable. No arbitrary age window is imposed: freshness
is anchored only by ordering against `preflight_inputs_frozen_at` plus the
Live precheck re-execution of the three probes (fresh double anchor). A
blocked DryRun may keep `status=pending` (skipped), and a blocked DryRun that
declares `status=pass` is fully validated exactly like a ready one (a blocked
DryRun can never hide a broken binding). The same ValidateDeclaredPass rule
applies to the independent review record on a blocked DryRun: a declared
`independent_review_record.status=pass` is fully validated (machine pass +
review pass runs the complete review mechanical gate), `status=pending` stays
allowed and skipped, and a declared-pass review on a pending/absent machine
confirmation is rejected outright (the review record binds the machine
confirmation hash, so a pending/absent machine side can never anchor it).
`TargetBindingConfirm` itself may consume a pending/absent object on a blocked
freeze.

Since `ADJ-20260810-0001` (C6), ready Live and ready DryRun additionally
require the **independent review record mechanical gate**: the freeze must
carry `independent_review_record.status=pass` bound to an out-of-repo review
record (`record_kind=e3-ready-freeze-review`, `is_evidence=false`,
`schema_version=1`, `exception=E3-PHYS-PREFLIGHT`, verdict `pass`, `blockers=0`,
`majors=0`, `reviewer_role`
matching `independent_reviewer_role` and differing from the operator role,
exact candidate pair/code/runner/`confirmation_contract_sha256` consistent,
and `machine_confirmation_sha256` equal to the machine confirmation record
SHA-256) plus a matching `.sha256` companion, and the review record must
satisfy the full time chain `machine confirmation ended_at <= review
started_at <= review ended_at <= final ready freeze
preflight_inputs_frozen_at` (the blocked confirmation freeze and the ready
draft keep a provisional/excluded `preflight_inputs_frozen_at`; the final
freeze is advanced after the review completes, and a review can never be
pre-filled before the machine confirmation completes). The review consumer
rejects unknown top-level fields (exact schema) like the confirmation
consumer. The self-declared
`independent_review_ready=true` boolean is only a static contract/role
readiness marker on a blocked confirmation freeze and never gates a ready
plan_status (a blocked confirmation freeze needs no review record). The sealed
complete record projects both bindings (status/roles/record_sha256/
record_path_sha256, anchored to the stable confirmation contract).

#### Two-phase confirmation contract (why records bind a stable projection)

`Get-FreezeContract` includes the governance/time field
`preflight_inputs_frozen_at`. The blocked confirmation freeze is frozen before
the machine confirmation runs (say `T1`), and the final ready freeze must be
frozen after the confirmation and review end times to satisfy the freshness
time gate (say `T2 > T1`). A confirmation/review record bound to the full
freeze contract would therefore be rejected by the ready-phase consumer: if
the ready freeze advances `preflight_inputs_frozen_at`, the full-contract hash
changes; if it does not, the time gate (`ended_at <= frozen_at`) fails.
`Get-ConfirmationContract` / `Get-ConfirmationContractSha256` resolve this by
projecting the phase-invariant core only: the execution core, the **exact
candidate pair** (`campaign_id` + `evidence_id`), external input hashes,
`code_sha`, runner, HDC, and roles, with `cleanup_baseline_frozen` /
`collection_ready` kept as static execution prerequisites. It excludes
`plan_status`, `preflight_inputs_frozen_at`, `machine_fresh_confirmation`,
`independent_review_record`, and `independent_review_ready` - the governance/
time fields that legitimately differ between the blocked and ready phases.
Workflow rule: the blocked freeze, the ready draft, and the final ready freeze
may each have a different **full** freeze contract hash (governance/time
fields differ), but their **confirmation contract hash must be byte-identical**
(otherwise the confirmation/review records cannot bind and the consumer
rejects). The producer (`-TargetBindingConfirm`), the ready consumer, and the
ready review record all verify the same `confirmation_contract_sha256` value.

Real Live requires `plan_status: ready`, a clean repository, manual operator
input, and the frozen target mapping. Normal revoke evidence comes only from the
planned visible UI/Settings actions; under `ADJ-20260808-0002`/`0003` scenario 5
uses machine-verified Settings app-info force-stop (one deterministic pre-action
`scenario-5-app-info` layout gate; the post-action capture is observation-only;
no manual confirmation - the old manual force-stop confirmation is historical
only), and the runner never issues HDC force-stop for it. Any exception/finally `force-stop`
is `notUsedAsRevoke`: it is residual cleanup only, followed by targeted
BundleDump and PidOf verification; HDC force-stop is restricted to the
`exception-cleanup` / `final-cleanup` reasons in the allowlist. Unknown
residual state remains blocked and is never
reported clean. The one initial Live invocation has been consumed. It started eight whitelisted
HDC processes for version/model/build preflight plus finally-targeted A/B
bundle, PID, and fixed staging probes. Model matched, but the visible live build
suffix drifted from the frozen build, so the runner stopped before continuous
capture, staging, installation, or any scenario; `campaign_started=false`, A/B
were never installed or run, and cleanup was verified clean. The independently
reviewed record is `reviewed-pass/blocked` with 0 blocker/0 major; reviewed-pass
means evidence review completion, not E3 pass.

The historical API23 initial-live plan became `blocked` with no
`infrastructure_reason`, so `infrastructure-blocked-retry-1` is not authorized
and prior campaign/evidence IDs must not be reused. After rebind,
`ADJ-20260806-0003`, host reverify PASS, `ADJ-20260806-0004` build confirmation
(`EV-E3-PHYS1BUILD7-20260806-0001`, build-confirm only), `ADJ-20260807-0001`
cross-day renumber, API26 0001 live `consumed-blocked`
(`E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`; no partial
scenario-5 replay; retained), and API26 0002 live `consumed-blocked`
(`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002`;
`reviewed-pass/blocked`; dual review 0 B/5 M; historical prepared
`E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001` remain
`superseded-unexecuted`), and `ADJ-20260807-0003` (host process terminal
probe + Settings app-info force-stop revoke path; runner/freeze example/selftest
updated with this commit), the governance `plan_status` at the
2026-08-08 host-remediation snapshot was `blocked-awaiting-device-authorization`.
The Windows signing/build host has
rebuilt the runner, freeze, and selftest snapshots and registered them as
[`EV-E3-PHYS1HOST-20260808-0001`](../../docs/evidence/e3-physical-preflight-host-remediation-2026-08-08.md)
(host selftest `HDC_PROCESSES=0`, independent review 0 B/0 M; the old
candidate pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` prepared
with freeze `plan_status: blocked`; DryRun `is_evidence: false`/HDC0/integrity
empty; old 20260807 candidate `INVALID-TIMELINE` unusable). That old candidate pair is now consumed by the external sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001` (record_status=collected / overall=blocked / verdict=blocked, execution_mode=live, is_evidence=true; campaign-seal SHA-256 `ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f` sealed_at 2026-08-08T09:53:23+08:00; hash-manifest SHA-256 `36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`; legacy-pair-consumption-audit `id-consumption-audit-1.txt` SHA-256 `b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`, 2026-08-10T10:43:09+08:00) and must never be reused. **Historical (2026-08-10 · `AUTH-E3-PHYS1API26-20260810-0002`)**: the user explicitly authorized a NEW full-whitelist campaign (see [`e3-physical-preflight-authorization-2026-08-10-0002.md`](../../docs/evidence/e3-physical-preflight-authorization-2026-08-10-0002.md); supersedes the consumed `AUTH-E3-PHYS1API26-20260810-0001`) with the new candidate pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`, `attempt=initial`/retry N/A, any gate failure stops with no retry and no ID switch, two new-pair consumption audits (audit-1 before any `-TargetBindingConfirm`, audit-2 after the final ready freeze; both hash recorded), and a single memory-only `hdc list targets` host-prep exception (exactly one token, process-scope `PHYS_1_TARGET`, no output/persistence, runner HDC whitelist NOT expanded). Live requires the machine fresh confirmation (`-TargetBindingConfirm`) and the review record bindings; user readiness attestation does not replace them. This registration banned HDC (except the single memory-only host-prep `hdc list targets`). **Current (2026-08-17):** `AUTH-E3-PHYS1API26-20260817-0002` binds the new pair `E3-PHYS-PREFLIGHT-20260817-0002` / `EV-E3-PHYS1API26-20260817-0002`, `attempt=initial` / retry N/A. It is `blocked-awaiting-full-gates`, not ready; its machine confirmation, review, freeze, DryRun, and Live are pending. Reviewer/source/HAP/S6 logic is unchanged. The 20260817-0001 pair is governance-operation-invalid retired at gate 9 after an unauthorized device process enumeration; its audit-2 file is not a valid gate pass, gates 10-13 were not run, and no DryRun, Live, or consumption occurred. Outside gate 4's exact host-prep and the runner's 22 allowlisted operations, all device commands, device process lists, and extra discovery are forbidden. Host HDC0 uses only absolute `/usr/bin/ps -eo comm=,args=` output and compares its first column. Historical AUTH/pairs cannot be reused. E3 remains open, E8 remains `CLOSED`, and NetBird or broader physical-device work remains forbidden.
