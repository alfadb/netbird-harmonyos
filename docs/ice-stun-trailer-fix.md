# ICE 直连失败根因：FINGERPRINT 校验边界与官方 `AttrCandidatePairID` 尾随属性不兼容

本增量同时把版本元数据从 `3 / 0.0.3` 提到 **`4 / 0.0.4`**（两处配置一致，json5 真实解析通过，见 §6）。§4 的两项修复已**实际落盘**到 `client/core/src/ice_session.rs`（FP 校验边界归一化 + `parse_stun` 的 MI 后解析护栏），另有第二个停摆缺陷一并修复（§4.4）。本次跑了全量 `cargo test --offline --locked`（**848 passed / 0 failed**，见 §7），并做了主机侧构建与签名。**没有**装机、**没有**真机运行、**没有**提交；真机直连是否恢复**仍未验证**。

## 1. 结论

官方 NetBird 主机（pion/ice v4 系）在 Binding request / success-response 的 FINGERPRINT **封口之后**追加 `AttrCandidatePairID`（`0x8100`，长度 65），并把报文头部的长度字段更新为**包含**该尾随属性；而 FINGERPRINT 的 CRC 仍是按**封口时**的长度值（FP 属性边界）算出来的。

本客户端 `verify_fingerprint` 用**整条报文的长度**作为 CRC 输入的长度字段，于是对这类报文必然算不出官方发送的 FP 值 → FP 门直接 `return`，入向 check 在**任何计数与日志之前**被静默丢弃。

同一份抓包逐包实算（§2.3）：

| 方向 | 报文数 | 现行口径（整条长度） | FP 边界口径 |
|---|---|---|---|
| 主机→手机（242 request + 40 success-response） | **282** | **0 通过** | **282 通过** |
| 手机→主机（FP 是最后一个属性） | 78 | 78 通过 | 78 通过 |

即这是**确定性的、只打击入向方向的兼容性拒绝**：不需要假设任何链路/网络故障，就能解释「主机发了 282 个包，手机侧一条入向记录都没有」。

## 2. 证据（离线、只读）

### 2.1 抓包与计数

- 抓包：`/home/worker/n2h-kit/status-20261001/ice-live-reconnect-20261005T230314Z/raw/host_n2h_ice2.pcap`（linktype 276，过滤 `udp and host 192.168.50.199`，窗口 2026-10-05T23:06:52.694Z → 23:08:10.671Z）。
- 共 360 个 STUN 报文：手机→主机 request **78**（12 个唯一 transaction = 6 轮 × 2 个本地端口）；主机→手机 request **242** + success-response **40**。
- 既有解码产物（同目录）：`analysis/stun_decode_tool.py`、`analysis/host-stun-decode-tight.txt`。

### 2.2 报文布局（282 个主机报文逐个实测，非抽样）

```
request:          USERNAME(0x0006) , ICE-CONTROLLING(0x802a) , PRIORITY(0x0024) , MESSAGE-INTEGRITY(0x0008) , FINGERPRINT(0x8028) , 0x8100(len=65)
success-response:                                              MESSAGE-INTEGRITY(0x0008) , FINGERPRINT(0x8028) , 0x8100(len=65)
```

- `0x8100` 位于 FP **之后**：282/282，无例外。
- 长度字段**覆盖**该尾随属性：282/282 满足 `报文 STUN 总长 == 头部长度字段`，FP 之后没有「超出声明长度」的字节。
- 因此现行实现读到的长度字段是被追加属性**改写后**的值，而 FP 的 CRC 是**改写前**（FP 边界）的值。

### 2.3 判定矩阵（CRC 逐包实算）

对每个主机报文取 FP 属性偏移 `fp_off` 与线上 FP 值，比较两种 CRC 输入：

- A（现行代码口径）：覆盖区间 `msg[..fp_off]`，长度字段 = `msg.len() - 20`（整条报文）
- B（FP 边界口径）：覆盖区间 `msg[..fp_off]`，长度字段 = `fp_off + 8 - 20`

| 方向 | 报文数 | A 通过 | B 通过 |
|---|---|---|---|
| 主机→手机 | 282 | 0 | **282** |
| 手机→主机 | 78 | 78 | 78 |

### 2.4 上游依据（外部查证）

`netbirdio/ice` commit `6202be846b51` 的 `candidatepair.go`：`sendSTUN` 在 `local.writeTo(msg.Raw, remote)` **之前**紧接一行 `msg.Add(AttrCandidatePairID, []byte(NewCandidatePairID(local, remote).String()))` —— 即封口后追加；属性常量名 `AttrCandidatePairID`，线上实测取值 `0x8100`、长度 65。与 §2.2/§2.3 的线上事实一致。

### 2.5 代码位置，以及「为什么手机日志什么都没有」

`client/core/src/ice_session.rs`：

- `verify_fingerprint`（663–674）：`let full_len = (msg.len() - HEADER_LEN) as u16;` —— 问题所在，应归一化到 FP 边界。
- 三个入向入口 FP 失败即 `return`：`on_check_request`（1474）、`on_check_response`（1630）、`on_error_response`（1681）。
- `on_check_request` 中 `inbound_reqs += 1`（1493）、`log_inbound_check`（1516）都在 FP 门**之后**；`N5d_ICE|check-rx`（1660）同样在其后。
  → FP 被拒 = 计数与日志**零痕迹**：与「OS 没投递 / 链路丢包」在手机日志上**完全不可区分**。
- `parse_stun`（558–638）：按声明长度一路走到末尾，属性可**互相覆盖**已记录字段，MI 之后没有任何停止点。MI 覆盖范围之外（MI 之后）的属性目前能覆盖 `username / priority / controlling / controlled / use_candidate / error_code / xor_mapped`，即**未认证字段可以顶掉已认证字段**。

## 3. 根因（确定）与未证明项

**确定（离线可复现，与抓包逐包吻合）**：现行 `verify_fingerprint` 对带 `0x8100` 尾随属性的官方报文必然校验失败；本客户端入向 ICE check 在这类报文上 **100% 被静默丢弃**。

**未证明（不得声称）**：

- 手机的 OS/NAT 是否把这 282 个报文投递到了应用进程：本抓包是**主机侧出口**证据，手机日志又因 §2.5 无法区分，两者都**未决**。
- 修复后真机 P2P 直连是否成功、是否还存在第二重故障（例如发往 `175.10.46.197:27498` 方向的候选、NAT 行为）。
- 本次**已**跑全量单测（§7）并完成主机侧构建/签名，但**未**装机、**未**真机验证；下一次安装尚未授权。

## 4. 修复实现（**已实施**：源码 + 单测/集成测试；真机未验证）

1. `verify_fingerprint`（`ice_session.rs:690`）：CRC 输入的长度字段归一化为 **FP 属性边界**（`fp_off + 8 - HEADER_LEN`），而不是整条报文长度；CRC 覆盖区间保持 `msg[..fp_off]`。**已实施**。
   - 兼容性：FP 在末位、无尾随的报文（手机自己发的 78/78）在该口径下**同样通过**，不引入回归。回归测试：`tests/ice_session_codec.rs` 的 `netbird_trailing_attr_*` 四个用例（含 FP 末位正对照与篡改反例）、`tests/ice_stun_trailer.rs` 的三个用例。
2. `parse_stun`（`ice_session.rs:561`）：见到 MESSAGE-INTEGRITY 之后，除**第一个 FINGERPRINT** 外忽略其余属性，并在该 FP 处停止遍历，使 MI 覆盖范围外的尾随字节无法覆盖已认证字段。**已实施**。
   - 这是**加固**项：只做第 1 条即可恢复互通（`0x8100` 是未知属性，会被跳过），第 2 条堵的是「未认证属性顶掉已认证字段」；`tests/ice_stun_trailer.rs` 用追加的 USERNAME/PRIORITY/role/USE-CANDIDATE、重复 MI、超出声明长度三类形状钉住。
3. `verify_message_integrity` 不受本问题影响（它用 MI 自身边界与 `+24` 长度），**未改动**。

### 4.4 第二个停摆缺陷：Answer + 零远端候选的僵尸谈判（**已修复**）

`peer_conn.rs::expire_dead_negotiation` 原先带 `remote_creds.is_none()` 守卫：远端一旦回过 Answer 就被当成「健康」，即使此后**一个远端候选都没有**也不再拆除。而 `maybe_start` 要求 `remote_creds` **且** ≥1 个远端候选，于是会话永远停在 `session = Some && started == false`：`run_once` 因 `!started` 立即返回、永不收包，socket 被永久占住（真机 round-230622244：07:09:03 发本地候选、07:09:04 收到 Answer、此后零候选，9 min 内没有任何重试）。

修复**去掉该守卫**：判据只看「未 start」，所有未 start 的会话都在 **`START_DEADLINE_MS` = 9 s**（3 × handshake retry）后判死并 `drop_session_rearm`，重武装沿用既有 **`RETRY_COOLDOWN_MS` = 2 s** 冷却后以全新凭证重新发起。即：远端静默 **>9 s** 才走到重发起；已应答过的 peer 收到过 Answer，不计入「远端静默」指数退避，只走既有 2 s 冷却节奏。

- 覆盖：`peer_conn.rs` 单测 `zombie_negotiation_times_out_and_reinitiates_with_fresh_credentials`（全量测试中 ok）。
- **未证明**：该僵尸期内**是否有数据被排队、这些数据的来源与去向**，本轮没有任何证据，不作声明。

## 5. 对既有报告叙述的更正

1. **`hgAfn…` 是主机（对端）的 ufrag，不是手机的。** 本客户端出向 USERNAME 按 RFC 8445 §7.2.2 组装为 `remote:local`（`ice_session.rs:1775`），入向门要求**第一段**等于本端 ufrag（1482）。抓包实测：手机→主机的 6 组 ufrag 对中 `hgAfn…` 恒在**第一段**（对端位），主机→手机中恒在**第二段**（发送者本位），两向自洽 ⇒ `hgAfn…` = 主机的 ufrag；每轮变化的那一段才是手机的 ufrag。`ice-live-reconnect-20261005T230314Z/README.md` §4.2 把 `hgAfn…` 标为「手机 ufrag」属**反向标注**。
2. **`check-rx = 0` / `reqs_rx = 0` 不等于「入向没到 / 链路丢失」。** 见 §2.5：FP 门在计数与日志之前静默返回；本条现在有**具体机制**（282/282 必然被拒），不再只是一种可能性。
3. **「主机 `.9` 上没有本地 UDP 27498 监听」不能否定外部 NAT 映射 `175.10.46.197:27498`。** 该端口是主机 srflx 候选的**外部**映射，NAT 外部端口不需要在本机 bind；主机自己也把同一个 `175.10.46.197:27498` 通告给别的 peer（`ice-sync-capture-20261005T171154Z/README.md` §5.3）。`ice-peer-comparison-20261005T1700Z/README.md` §5 由「无本地监听」推出「手机发往 27498 的 check 今天不可能被主机应答」不成立。
4. **身份**：手机自身 WG 公钥 `nQ334olB…`、NetBird IP `100.108.142.138`；主机 `t7tldQA3…`（LAN `192.168.50.9:34486`）；`TrBfURPG…`（netbird-ohos，`100.108.19.58`）是**另一个 peer**，不是这台手机的自身 key。

> 本文只保留标识符前缀，不记录完整 ufrag/公钥/口令，也不复制抓包内容。

## 6. 版本元数据（本增量）与复现

- `client/build-profile.json5`（`app.products[name=default]`）与 `client/AppScope/app.json5` 两处的 `versionCode`/`versionName`：`3 / 0.0.3` → **`4 / 0.0.4`**，取值一致（权威值口径见 `docs/app-install-version-policy.md`）。
- 校验：python3 的 `json5` **真实解析**两个文件通过，读出 `versionCode=4`、`versionName=0.0.4`。已用 `bash client/build.sh` 构建（未签名 HAP）并用 `verify-app` 校验打包版本 `code=4 / name=0.0.4`、apiVersion target/compatible=26（见 §7）；**未**装机。

复现判定矩阵（只读，离线；只打印计数，不输出任何事务号/凭据）：

```python
import struct, zlib
data = open("<上文的 host_n2h_ice2.pcap>", "rb").read()
lnk = struct.unpack("<I", data[20:24])[0]; off = 24; pkts = []
while off + 16 <= len(data):
    ts, tu, incl, _ = struct.unpack("<IIII", data[off:off+16]); off += 16
    f = data[off:off+incl]; off += incl
    l3 = f[20:] if lnk == 276 else (f[16:] if lnk == 113 else f[14:])
    if l3[9] != 17: continue
    udp = l3[(l3[0] & 0xF) * 4:]; sp, dp, ul, _ = struct.unpack(">HHHH", udp[:8])
    p = udp[8:8+ul-8]
    if len(p) >= 20 and p[4:8] == b"\x21\x12\xa4\x42": pkts.append(("P" if l3[12:16] == bytes([192,168,50,199]) else "H", p))
def crc(p, length_field, boundary):
    b = bytearray(p[:boundary]); b[2:4] = struct.pack(">H", length_field)
    return zlib.crc32(bytes(b)) ^ 0x5354554E
tally = {}
for side, p in pkts:
    n = struct.unpack(">H", p[2:4])[0]; o = 20; fp = None
    while o + 4 <= 20 + n:
        at, al = struct.unpack(">HH", p[o:o+4])
        if at == 0x8028: fp = (o, struct.unpack(">I", p[o+4:o+8])[0])
        o += 4 + al + ((4 - al % 4) % 4)
    if not fp: continue
    a = crc(p, len(p) - 20, fp[0]) == fp[1]
    b = crc(p, fp[0] + 8 - 20, fp[0]) == fp[1]
    k = (side, a, b); tally[k] = tally.get(k, 0) + 1
print(tally)   # 期望 ('H', False, True): 282 与 ('P', True, True): 78
```

## 7. 本次验证（主机侧，无设备；产物目录 `/home/worker/n2h-kit/app-builds/ice-fix-v004-20261006-073102`）

- 全量 `cargo test --offline --locked`（`CARGO_HOME=/home/worker/rust/cargo`、`RUSTUP_HOME=/home/worker/rust/rustup`、`CARGO_TARGET_DIR=/home/worker/n2h-kit/cargo-target-0034`、`PATH` 前缀 `cargo/bin`）：**exit 0**，45 个测试二进制，**848 passed / 0 failed**。本修复相关用例全绿：`tests/ice_session_codec.rs` 的 `netbird_trailing_attr_*`（4）、`tests/ice_stun_trailer.rs` 的 `official_candidate_pair_id_trailer_*` 与 `fingerprint_computed_over_a_trailer_length_is_rejected`（3）、`peer_conn.rs` 的 `zombie_negotiation_times_out_and_reinitiates_with_fresh_credentials`。
- 构建：`DEVECO_SDK_HOME=/home/worker/harmonyos/command-line-tools/current bash client/build.sh` → **exit 0**；未签名 HAP `5242143` bytes / sha256 `00bcd740cc96b8f269916d53a0b842bc84bb4b5de12b17bd0c4cd63abbb201fc`；`pack.info` version `code=4 name=0.0.4`、apiVersion `target=26 compatible=26`、releaseType `Release`；`libs/arm64-v8a/libnetbird_core.so` 位于 HAP 内（未签名与已签名两侧分别复算一致），llvm-strip 后 sha256 `e012064e9a17e59c181bcebd44901a12f7151d0af2e70efcc1e8795af9623166` == core 产物；native 自识别 `N2H_BUILD_SHA256=aa523d58b8f80f1d4e1ed6b1b256e5473e6f51927473602ef590ecb55332897a`（独立复算，内嵌于产物）。
- 签名（launcher 精确 8 参数；口令只传文件路径，不读取不打印）：**exit 0**，`signApp errCode=SUCCESS_CODE`；签名 HAP `5331007` bytes / sha256 `0c62855f6b4631444b3278c6b5c68f7ca791e0a2e1ebed1c94721d60264446a1`。
- `verify-app -inFile <signed> -outCertChain vc.cer -outProfile vp.p7b`：**exit 0**，`Digest verify result: true (SHA-256)`、`verify: Verify success`、`verify-app success`。
- **未**装机、**未**真机运行、**未**提交/推送；全程未调用 hdc/adb，无设备副作用。真机直连是否恢复**仍未验证**；§4.4 僵尸期内的数据排队情况无证据。
