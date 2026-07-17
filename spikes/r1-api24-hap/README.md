# E2 C Network API 24 HAP Probe

This directory is a short-lived HarmonyOS research probe for the API 24 x86_64 Emulator gates. It must not evolve into a product shell.

## Current Scope

The current build graph is an ordinary-application E1 regression plus E2 C-network probe:

- one unsigned API 24 Stage application HAP with bundle `cn.alfadb.netbird.r1probe`;
- one ordinary `EntryAbility`; the E2 evidence runner does not build, install, or invoke the TestRunner sidecar;
- the complete E1 C-only 10-round buffer, pthread callback, fd ownership, and resource regression before E2 starts;
- an independent `libe2network.so` using libc TCP, UDP, `getaddrinfo`, `poll`, and `epoll` APIs;
- 10 E2 rounds per PID and three distinct cold-start PIDs;
- a visible PID-specific `E2 C network PASS` or FAIL page.

The current HAP contains no NetBird, Go runtime, TUN, VPN, Native Child invocation, `libgoprobe.so`, or `libtls-*` input. Historical Go/TLS/PS4 sources and TestRunner execution are outside this probe. Generated libraries, HAPs, and build state are ignored, and there is intentionally no signing configuration.

## E2 Implementation

Every E2 round performs all of these operations in native code:

1. Create a new nonblocking IPv4 TCP listener on loopback, connect and accept through `epoll`, send a deterministic 4096-byte payload in fixed chunks, force 142/179 partial reads on the two directions, echo and compare FNV-1a hashes.
2. Observe an empty 25 ms `poll` timeout, close the peer and require `epoll` plus EOF, close the listener and require `EBADF(9)`, then reconnect to the old port and require `ECONNREFUSED(111)`.
3. Create two new UDP loopback endpoints and exchange three fixed-ID/hash datagrams in each direction, validating the complete datagram, source address, source port, ID, payload, and hash.
4. Discover the ordinary process IPv4 network and mask through `getifaddrs`, derive host candidates from that measured network, and connect to the Pod-local temporary C service through the Emulator NAT address that actually succeeds.
5. Echo one deterministic 2048-byte TCP payload and three fixed-ID/hash UDP datagrams against host ports `39021/39022`. The host service independently logs PID, round, ID, byte count, hash, partial I/O, and final per-PID counts.
6. Resolve `localhost` through C `getaddrinfo` and require only loopback results. For `e2-reserved.invalid`, AI_NUMERICHOST对非数字名确定性EAI_NONAME，不经过resolver；this error path cannot send a public DNS query.
7. Compare pre/post `/proc/self/fd` and `/proc/self/task` snapshots with the stabilized E2 baseline. Any growth, timeout mismatch, I/O mismatch, missing event, unexpected DNS result, crash, or marker count fails closed.

The temporary host server is compiled from `e2-host-server.c`, binds only `127.0.0.1`, accepts exactly 30 TCP exchanges and 90 UDP datagrams from exactly three PIDs, then exits. The runner separately records the guest `/proc/net/route` table before installation. The application does not assume a universal NAT address and only reports the route-derived candidate that passed real TCP and UDP traffic.

## Clean Build

Use fixed tool paths rather than the floating `current` symlink:

```bash
cd /home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
```

Expected output:

```text
entry/build/default/outputs/default/entry-default-unsigned.hap
```

Hvigor's unsigned packaging is not byte-for-byte reproducible in this environment. The evidence runner archives the exact HAP before installation and binds conclusions to that SHA-256.

## Emulator Run

The runner performs a clean build, compiles and starts the loopback-only host service, cold-starts the visible `netbird_api24_phone` instance, sets guest HiLog buffers to 16 MiB, installs only the application HAP, and executes three ordinary EntryAbility cold starts:

```bash
cd /home/worker/work/base/netbird-harmonyos
bash spikes/r1-api24-hap/e2-c-emulator-run.sh
```

The run is local-only. It must not be given public endpoints, and it does not use Go, NetBird, PS4, TestRunner, or a physical device.

The runner fails closed unless all of these conditions hold:

- the HAP has both x86_64 native members, no historical Go/TLS/TestRunner member, and the E2 ELF references the required public C network symbols;
- the host TCP and UDP sockets are bound only to Pod loopback;
- current qemu boot-complete and Beta HDC shell readiness pass before installation;
- the installed package is a normal, non-system application whose main element is `EntryAbility`;
- all three cold starts have distinct nonempty PIDs;
- every PID has a complete E1 regression and exactly 10 PASS records for each E2 subsystem;
- host raw logs have exactly 10 TCP and 30 UDP records for each measured PID;
- every E2 resource line reports exact fd/thread restoration;
- each PID remains alive through a visible 1320x2856 E2 PASS screenshot;
- complete unfiltered HiLog, tag HiLog, host log, fault list, source inputs, two native members, host binary, HAP, transcript, Emulator console, and screenshots are archived and hashed;
- force-stop, uninstall, bundle absence, Emulator/HDC shutdown, host exit, tested-port cleanup, `bash -n`, markdownlint, diff check, sensitive scan, and public-endpoint source scan all pass.

## Measured Result

`EV-E2-EMU24-20260717-0002` completed with `record_status: reviewed-pass` and `verdict: pass`; E2 is closed and E3 is next:

| Run | PID/main TID | E1 rounds | E2 rounds | TCP loop | UDP loop datagrams | Host TCP/UDP | Stable fd/thread | Screenshot |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 1 | 2992 | 10 | 10 | 10 | 60 | 10/30 | `38/29` | visible E2 PASS |
| 2 | 3155 | 10 | 10 | 10 | 60 | 10/30 | `38/28` | visible E2 PASS |
| 3 | 3351 | 10 | 10 | 10 | 60 | 10/30 | `38/29` | visible E2 PASS |

The measured HAP SHA-256 is `76004031e937604b878049728569d0163b4a2e2e987ceb61eba4de86969baf4f`; the executed x86_64 E2 member is `7e14cfafc6fad8234be193c6787929c4042eb7d8822a9c2fee5804e67e26151a`. The guest route and ordinary-process interface both identified `10.0.2.0/24`; the first route-derived candidate `10.0.2.2` completed all traffic to the Pod-local loopback-bound service.

Independent review is complete: E2 is closed with `reviewed-pass/pass`, and E3 is next. E1 remains blocked overall because the latest formal NetBird baseline's official Go 1.25.12 loader path still fails. E8 remains `CLOSED`, physical-device execution remains prohibited, and no R stage, arm64, VPN, channel, public-network, or product claim follows.

## Evidence

- [E2 C-network evidence record](../../docs/evidence/e2-c-network-api24-emulator-2026-07-17.md)
- [E1 C-only evidence record](../../docs/evidence/e1-c-bridge-api24-emulator-2026-07-17.md)
- [E0 ordinary-application evidence record](../../docs/evidence/e0-api24-emulator-2026-07-17.md)
- Exact HAP/native/host/source archives and hashes under `docs/evidence/raw/EV-E2-EMU24-20260717-0002-*`
- Complete replay transcript, tag HiLog, three unfiltered HiLogs, host raw log, fault list, Emulator console, and screenshots under the same prefix

`EV-E2-EMU24-20260717-0001` is retained as a failed attempt. Its ordinary process completed E1 and the first loopback operations but received `EACCES(13)` reading `/proc/net/route`; `0002` uses ordinary-process `getifaddrs` while preserving the guest route as separate runner evidence. The failed ID was not reused.
