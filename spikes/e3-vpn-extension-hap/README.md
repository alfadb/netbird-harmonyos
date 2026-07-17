# E3 VPN Extension API 24 HAP Probe

This directory is an isolated, short-lived E3 probe for the API 24 x86_64
phone Emulator. It does not share a build graph with the E0-E2 evidence
project and must not evolve into a product application.

## Scope

The project builds the same minimal Stage source as two ordinary application
products with independent identities:

- A: `cn.alfadb.netbird.e3vpna`;
- B: `cn.alfadb.netbird.e3vpnb`;
- one normal exported `EntryAbility` with visible Start VPN and Stop VPN
  buttons;
- one non-exported `E3VpnExtensionAbility` declared with manifest type `vpn`;
- only the ordinary `ohos.permission.INTERNET` permission.

The UI calls only the API 24 public contracts
`@ohos.net.vpnExtension.startVpnExtensionAbility` and
`stopVpnExtensionAbility`. The Extension inherits the API 24 public
`@ohos.app.ability.VpnExtensionAbility` contract and records only `onCreate`
and `onDestroy`. The source does not create a `VpnConnection` and does not call
its `create`, `protect`, or `destroy` methods; those operations remain E4/E5
work. API 24 has no authorization observer contract, so the probe records the
actual start/stop promise settlement and lifecycle without inventing an
observer.

The probe contains no Go, NetBird, PS4, native library, TestRunner,
`MANAGE_VPN`, permission grant, system/debug/enterprise bypass, hidden service
invocation, policy change, public endpoint, or physical-device operation.

## Build

From this directory, build A and B independently:

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

Each evidence run archives the exact A HAP before cleaning for B, then archives
the exact B HAP before installation. The two unsigned research HAPs are not
release artifacts.

## Emulator Run

Run the bounded evidence workflow in tmux from the repository root:

```bash
tmux new-session -d -s e3-vpn-extension \
  "bash spikes/e3-vpn-extension-hap/e3-vpn-extension-emulator-run.sh"
tmux capture-pane -pt e3-vpn-extension
```

The runner uses only normal `EntryAbility` actions and `hdc uitest` layout,
click, swipe, key, and screenshot operations as simulated user input. It
records three A authorization attempts with distinct PIDs, one fresh B denial
attempt, a normal UI stop observation, an A/B conflict observation, and a
Settings path opened by clicking its desktop icon. Complete unfiltered HiLog,
tag HiLog, layouts, screenshots, fault listings, source/HAP hashes, build logs,
and Emulator console output are archived under the evidence ID.

Force-stop is used only to terminate a blocked scenario before the next clean
PID and is explicitly logged as cleanup. It is never classified as user
revocation. Uninstall is performed only after all scenarios as final cleanup.

## Measured Result

`EV-E3-EMU24-20260717-0003` and read-only supplement `0004` are both
`record_status: reviewed-pass` with `verdict: blocked`. Independent review
covers evidence integrity only. A normal Start VPN button emitted `UI_START` in A PIDs
3148, 3322, and 3605 and fresh B PID 3857. On every request the system tried
to start `com.huawei.hmos.vpndialog/VpnServiceExtAbility`, but the API 24
Emulator image reported that the bundle and Extension did not exist. No
authorization window appeared, every start promise remained pending for the
10-second observation window, and `E3VpnExtensionAbility.onCreate` did not
run.

The missing first authorization UI means the run cannot supply an allow click,
deny click, active-A stop/onDestroy, ordinary Settings revocation, or active-A/B
conflict. The three A PIDs reproduce the blocked request; they do not count as
three allow paths or prove authorization persistence. A normal UI stop request
also remained pending because A never became active. Conflict requests from A
PID 4826 and B PID 5206 each emitted `UI_START`, but neither produced
`onCreate`; the probe did not call `VpnConnection.create` to manufacture a
create-level result.

The runner opened the real `com.huawei.hmos.settings` page by clicking the
ordinary desktop Settings icon. The complete Settings main layout contained
WLAN, System, Apps & services, and other entries, but no VPN management or More
connections entry. The three root causes are the exact image's missing
`com.huawei.hmos.vpndialog`, no bypass through the ordinary public API, and no
ordinary Settings VPN entry. Force-stop remained scenario cleanup only and
uninstall remained final cleanup; neither was classified as revocation.

`EV-E3-EMU24-20260717-0001` is retained as the first incomplete blocked
exploration. `EV-E3-EMU24-20260717-0002` is retained as
`invalidated/fail`: its Settings selector clicked a non-clickable text label,
leaving Home and purported Settings captures identical. ID 0003 uses a
clickable icon selector and bundle/title/layout-change assertions.

E3 remains open and E4-E7 are dependency blocked and do not start. All
currently reachable work that does not depend on Go, namely E0, E1-C and E2,
is complete; the official E1 Go path remains blocked. E8 remains `CLOSED`,
physical-device execution remains prohibited, and the Emulator limitation is
not extrapolated to other architectures, named physical devices, or commercial
HarmonyOS.

## Evidence

- [E3 VPN Extension API 24 Emulator record](../../docs/evidence/e3-vpn-extension-api24-emulator-2026-07-17.md)
- Exact HAPs, source archive, full unfiltered HiLogs, layouts, screenshots,
  fault list, Emulator console, transcript, and artifact manifest under
  `docs/evidence/raw/EV-E3-EMU24-20260717-0003-*`
