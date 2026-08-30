# Go 1.27.0 stock c-shared ELF TLS 对照研究（G0 后续问答）

最后核验：2026-08-30

**record_status: collected** / **verdict: research-only**（`RS-G0-GO127ELF-20260830-0001`）

## 背景

G0 live（`EV-G0PHYS1API26-20260830-0001`，`reviewed-pass/blocked`）实测冻结物理元组 loader 拒绝 stock Go 1.25.12 arm64 c-shared 后，用户提出"升级到最新 Go 是否可行"。本记录以当日最新稳定版 **go1.27.0**（go.dev/dl 实测确认；前一稳定线 1.26.7）做同源 host-only 对照，方法沿用 `RS-E1-GO126ELF-20260809-0001`。

## 方法（host-only，无设备）

- go1.27.0 linux/amd64 官方 tarball（sha256 `675c26c4…0685` 校验通过）解压于 `/tmp`（一次性，非持久安装）
- 同一探针源码（G0 `probe.go` 逐字）+ 同一 OHOS `aarch64-unknown-linux-ohos-clang`，`GOTOOLCHAIN=local GOOS=linux GOARCH=arm64 CGO_ENABLED=1 -trimpath -buildmode=c-shared`
- `llvm-readelf` 检查 TLS 重定位与动态段

## 结果

go1.27.0 产物（`/tmp/g0-127/libgoprobe127.so`，2058416 字节）TLS 剖面与 1.25.12 **结构逐项相同**：

- `R_AARCH64_TLS_TPREL64` **恰 1 条**（symbol index 0、addend 0，与 G0 预登记剖面同形）
- Dynamic `FLAGS=SYMBOLIC BIND_NOW`（无 STATIC_TLS）、`FLAGS_1=NOW NODELETE`
- `NEEDED` 仅 `libc.so`；导入 `pthread_create`；导出 `Hello`/`RuntimeProbe`

即：**1.25.12 → 1.26.0 → 1.27.0 三个版本的 c-shared 均为 initial-exec TLS 形态，未发生任何模型变化**。

## 上游状态（2026-08-30 查证）

- [golang/go#71953](https://github.com/golang/go/issues/71953)（proposal: support general dynamic TLS model）仍为 **Open**、Milestone=Proposal、无到期；原型 CL 644975 未合入。[#48596](https://github.com/golang/go/issues/48596) 中"probably in Go 1.26"的预期未兑现（1.26.x–1.27.0 均未落地）。
- 相关历史议题：[#13492](https://github.com/golang/go/issues/13492)（musl c-shared dlopen，2015 年起）、[#54805](https://github.com/golang/go/issues/54805)。

## 判读（按既有治理）

- 升级 Go（至 1.27.0）**不改变**物理 loader 拒绝结论：设备拒绝的是 IE TLS 重定位形态本身，而该形态在最新版中逐字未变。
- 治理触发条件不变（N0 决议第 7/8 条）：仅当**正式 Go release 合入 dynamic TLS 支持**或 **NetBird 正式采用新工具链**时，重跑对应 E1/G0 类测量；本记录不构成该触发（上游仍为未合入 proposal）。
- 本记录为 research-only：不占用 evidence ID、不改变任何门状态、不构成升级建议。
