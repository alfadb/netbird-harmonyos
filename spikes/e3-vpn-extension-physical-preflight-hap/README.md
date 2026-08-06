# E3 Physical VPN Extension Preflight

This is an isolated local-preparation project for the frozen HarmonyOS 6.1.0
API 23 PLA-AL10 E3-PHYS-PREFLIGHT. It does not replace or modify the historical
API 24 Emulator probe in `../e3-vpn-extension-hap/`, does not modify historical
raw evidence, and does not create physical-device evidence.

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
its matching id. Stop without an active id emits `UI_STOP_SKIPPED` and does not
invent an id. A successful Stop may clear the matching id; that does not
itself permit the next scenario to begin.

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

## API Mapping

The Stable Command Line Tools 6.1.1.290 SDK declares HarmonyOS 6.1.1/API 24.
Both products compile against `6.1.1(24)` while declaring target and compatible
SDK `6.1.0(23)` for the frozen device contract. Stable Hvigor 6.24.3 requires
one product literally named `default`, so logical `vpnA` uses that name.

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
material, endpoint data, or current secret values. Its required field groups
are campaign identity/status/retry, the exact target tuple and 60-second window,
settings re-allow expected path defaulting to `direct-system-activation` with
required `settings_reallow_path_policy: observation-only`, ordinary-development
signing, final A/B artifact hashes, frozen source archive/manifest, SDK input
map, HDC version/hash, runner/code hashes, freeze time, cleanup/collection/review
Boolean gates, and distinct operator and reviewer roles. The path policy is part
of the freeze contract hash; any value other than `observation-only` is rejected.
Scenario 5 records `settings_reallow_path` expected/actual/match/observation and
never blocks solely because actual path differs from the predicted path; pass
still requires A re-activation with `VPN_ONCREATE`/create-fd markers, ordinary
Settings revoke, destroy terminal, and post-destroy fd cleanup.
`signing.device_in_profile` is JSON Boolean `true` in the example to show the
required type; the adjacent placeholder basis explicitly means it is not a
checked-in device-profile claim. Device HiLog zone mapping (`CST=>+08:00`) and
the 3-second device-clock skew tolerance are runner constants, not
freeze-manifest fields; they appear only on the emitted record's `clock_source`.

After HDC/device precheck, Live starts one continuous campaign HiLog process and
records an initial complete-line/byte anchor. Scenarios never restart that
stream. Each scenario waits for operator READY first (READY latency is excluded
from the measured window), then records a fresh byte anchor after READY and
before the action prompt, filters only new complete lines by parsed
`device_observed_at` from action prompt through the actual observation end,
retains `host_observed_at`, and requires measured healthy coverage through at
least 60 seconds after ACK. Install is three-state: only explicit rejection
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
captures the visible system authorization UI (screenshot + layout) after the
operator confirms it is visible and before Allow; capture failure blocks
scenario 2 and keeps the artifact reference.

`EvidenceRoot` contains only the redacted structured projection, record,
operator attestation, collection manifest, lock, and seal. Unfiltered HiLog,
screenshots, layouts, and targeted A/B fault outputs go to an independent
out-of-repository `RawRoot` sibling (by default `<EvidenceRoot>.raw`), never to
`EvidenceRoot` or this repository. There is no raw transcript:
`projection/transcript.redacted.jsonl` is the only transcript. Recursive leaf
redaction occurs before JSON serialization, and every transcript hash binds the
re-canonicalized human-readable payload. Live runner records may be `collected`,
`blocked`, or `invalidated`; reviewed states remain exclusive to the independent
review step.

Run the host-only fixture suite. It parses both scripts and exercises a blocked
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
`LiveSimulation` always records `is_evidence: false`, `record_status: blocked`,
and `verdict: blocked`, including functional-error and integrity-tamper paths.
A retry prior must instead be a frozen matching Live blocked evidence record.

Real Live requires `plan_status: ready`, a clean repository, manual operator
input, and the frozen target mapping. Normal revoke evidence comes only from the
planned visible UI/Settings actions. Any exception/finally `force-stop` is
`notUsedAsRevoke`: it is residual cleanup only, followed by targeted BundleDump
and PidOf verification. Unknown residual state remains blocked and is never
reported clean. The governance plan is now `ready`, which means only that the
single campaign input is ready; no campaign install/run has occurred and no
record status or verdict exists. This README does not independently authorize a
Live invocation: use the dedicated governance plan and out-of-repository freeze.
E8 remains `CLOSED`, and NetBird or any broader physical-device work remains
forbidden.
