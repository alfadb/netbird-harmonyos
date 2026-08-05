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

The HAPs are unsigned local preflight artifacts. Do not sign or install them.
A later governed physical-device procedure must separately review the source,
unsigned artifacts, hashes, signing boundary, operator steps, and evidence plan
before any installation can be considered. This audit does not invoke HDC,
sign, install, start an Emulator, contact a physical device, log in, download
dependencies, or write evidence/governance files.
