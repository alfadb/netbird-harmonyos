# dstID 推导交叉验证 — 用官方客户端实测数据检验 relay 对端 peer ID（2026-09-15）

数据源：`/home/worker/harmonyos-signing/netbird-n1bdisc/records/oracle-official-client-20260914/`（仓B，只读）。
本任务只读交叉验证，未改任何代码、未运行设备命令、未联网；本文件是本次唯一新建/修改的文件。

## §1 结论

**一致。官方客户端的 relay peer ID 推导与我方实现完全相同：`"sha-" || SHA256(对端 WG 公钥的 base64 字符串的 ASCII 字节)`，共 36 字节（线上原始字节）；人类可读形 `"sha-" + base64Std(hash)` 与官方日志逐字符一致。13/13 个官方日志中出现的 sha- ID 全部逐字符命中，0 个例外——"dstID 推导不一致导致中继静默丢弃"这一假设可排除。**

置信度：**高**。依据三重独立证据：(a) 官方客户端在单行日志里同时给出公钥明文与其 hashedID（2 例，最强）；(b) 同日志毫秒级相邻的公钥→ID 时间线配对（19 对，全部命中）；(c) 状态文件全部 20 个公钥的独立计算与日志观测集合精确吻合（13 命中 / 7 无日志证据，无任何反例）。

## §2 我方公式与实现位置（只读核对）

- `client/core/src/relay.rs:191-197` `PeerId::from_wg_pubkey_string`：`sha256(pub_key_b64.as_bytes())` + 前缀 `sha-`（`relay.rs:99-102`：`PEER_ID_SIZE=36`、`PEER_ID_PREFIX=b"sha-"`）；`relay.rs:219-221` `readable()` = `"sha-" + base64Std(32B hash)`，仅日志用。
- golden 向量 `relay.rs:595-598`：公钥 `AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=` → `sha-sBGY+A11I1BRB1Oa69VbCNGSO/2R9ySDpkTP30DFEUI=`（测试 `relay.rs:624-631`）。本次用 python3 hashlib 独立复算：hex `b01198f80d7523505107539aebd55b08d1923bfd91f72483a644cfdf40c51142`，可读形与 golden **逐字符一致**（计算方法自证）。
- 发往对端的 dstID 调用点：`client/core/src/connector.rs:2026-2027`（`ensure_lane` → `open_conn(&pid)`）、`connector.rs:1989`、`client/core/src/relay_client.rs:1325`。
- 规格：`docs/relay-client-spec-20260914.md:125-133`（§3.2 推导）、`:158-168`（§4.2 `dstID = HashID(dstPeerID)`；服务端转发时把帧内 36B 改写为发送方 ID）、`:269`（读侧语义）。
- 上游依据（参考检出 `netbird-n1bdisc/refs/netbird-791401060d2b/`）：`shared/relay/messages/id.go:15,25-31`（`prefix=[]byte("sha-")`、`HashID` 对 `[]byte(peerID)` 字符串做 sha256）、`id.go:20-22`（`String()` = 前 4 原始字节 + `base64.StdEncoding`）；`client/internal/connect.go:425`（`NewManager(..., myPrivateKey.PublicKey().String(), ...)` —— 被哈希的是 WG 公钥的 base64 **字符串**，非解码字节）；`shared/relay/client/manager.go:112-124,358`（peerID 原样下传）；`shared/relay/client/client.go:233,302`（`messages.HashID`）；`shared/relay/messages/message.go:220-229`（Transport 帧单一 36B ID = dst）；`relay/server/peer.go:223`（服务端改写为发送方 ID）。

## §3 官方实测证据

实测环境：官方 daemon **0.77.1**（`status-d-netrpi.txt:337-338`），relay `rels://home.alfadb.cn:28443`（生产自托管，relay 0.76.3，`HANDOFF.md` §头）。
计算方法（python3）：`"sha-" + base64.b64encode(hashlib.sha256(公钥字符串.encode()).digest()).decode()`。

证据类别：
- **[A] 单行直证**（最强，无配对推断）：`create new relay connection: local peerID: <公钥>, local peer hashedID: sha-…`。
- **[B] 时间线相邻配对**：`manager.go` 日志 `open peer connection via permanent server: <公钥>` 后 ≤20ms 同日志出现 `prepare the relayed connection, waiting for remote peer: sha-…`（上游同一代码路径 `client.go:302→305-306` 的两个日志点）。自动核验 **19 对，19 命中 / 0 失配**。
- **[C] 集合吻合**：日志中全部 13 个互不相同的 sha- ID 均能由状态文件 20 个公钥按我方公式算出；无未命中观测值。

| # | 对端 | 公钥（base64 字符串，原文） | 官方 ID（日志原文） | 我方计算 ID | 一致? | 配对依据 / 不确定度 |
|---|------|------------------------------|----------------------|-------------|-------|----------------------|
| 1 | netrpi（本机） | `F0GVt4oEj2ZGaHNqXjcotHHlsz3qhBHBm1sfou+e0DQ=` | `sha-27MDvCryE04wbNBnCifkvLIOAnBqfhhM42r/DiK7Cp0=` | 同左 | ✅ | [A] `netbird-debug-netrpi.log:137`；公钥另见 `status-d-netcenter.txt:36`（peer netrpi）。不确定度：无（单行直证） |
| 2 | netcenter（本机） | `ftUD3aGWs/FmwtK3D44fnZAQatiO+xSfW/1zdHAkdkI=` | `sha-LICqXrzbd23hUhgZ48ehPylwY/UU3W9riI8MF7cjG+w=` | 同左 | ✅ | [A] `netbird-debug-netcenter.log:95`；公钥另见 `status-d-netrpi.txt:260`（peer netcenter）。不确定度：无 |
| 3 | nethostdebian | `t7tldQA3k8xRALQbb66LfuGnYNt9U78Ah9X/S7L3PCg=` | `sha-lHWo+w28AK/Ek8CuHHDvJWkY1XoXz/gRYfiqv/Ci0qs=` | 同左 | ✅ | [B] `netbird-debug-netrpi.log:415→422`；公钥 `status-d-netrpi.txt:180`。弱依赖同一日志相邻性，低 |
| 4 | blkinternet | `sNerYoiBJtRh6jjixN8q1Vc4bw4kEAViSSPC57Tw6hM=` | `sha-RoRvB5M/gvWdwOZpS735xVREPpihG8WdJ5YrGTPbiIk=` | 同左 | ✅ | [B] netrpi.log:373→374（同毫秒）；公钥 `status-d-netrpi.txt:212`。低 |
| 5 | kenonenc | `jv2IfmjFaD1rQojCNfEp3fCRECpLNCv3Iq0N+EdCgzU=` | `sha-G3oxcuE/KTbNWt+k7guRJMbx8ZMrSXyFfpb2GZy/aDA=` | 同左 | ✅ | [B] netrpi.log:453→454；公钥 `status-d-netrpi.txt:116`。低 |
| 6 | proxyhk | `C2Qhc+YxVWWhhZYvMkYA+rywjuu2aBMrfl0IFhFsLXM=` | `sha-6t4KOtQp3nn019aziZsGgRzUcy40Zbf+NjLUdi0kVZ4=` | 同左 | ✅ | [B] netrpi.log:468→469；公钥 `status-d-netrpi.txt:228`。低 |
| 7 | scmzblhost | `XAX0DJicDhbFrszMobbUAC38lUkseErJysVcRfYdZWk=` | `sha-HYnt8Lovj6dYiXy42YlFPphISbgs0Q9Jxq/ZwA/pqeE=` | 同左 | ✅ | [B] netrpi.log:481→482；公钥 `status-d-netrpi.txt:244`。低 |
| 8 | devchenziran | `BAVlLzPqF80UH0ZHnhs8JAPF2gfwsczQXPyFqiREiDk=` | `sha-udlcE9Az6ecREnQllD4gHASfHdDBRc79N2GWv/ZqO+s=` | 同左 | ✅ | [B] netrpi.log:542→543；公钥 `status-d-netrpi.txt:276`。低 |
| 9 | gymesserver | `n4ZafquyJtd9M1ljePoGTUjnrmzFf4oorD8dlVeRrW8=` | `sha-ncY4daMTAvPS7DXbMjfbZB9rLS395EgbYsU25ZDpq2M=` | 同左 | ✅ | [B] netrpi.log:553→554（另 2550→2551、2632→2634）；公钥 `status-d-netrpi.txt:52`。低 |
| 10 | blkserver | `CJzDh5BrUGwcJnjpI9BbSMlngslroUkIJk7e/gYtzHg=` | `sha-9RlLHhCqav5zQKDi3aGZKazLsE3gmPPtLgAeXxngYZM=` | 同左 | ✅ | [B] netrpi.log:626→627；公钥 `status-d-netrpi.txt:196`。低 |
| 11 | proxyusa | `HkPMIwdut1F/0ke+w6kvbxaxbF0fMSisTMJ2SSb8qns=` | `sha-E/KNrF2/Je5SmYYFPC9mWYKCbnF+dTKnWfvs0Vz5skI=` | 同左 | ✅ | [B] netrpi.log:708→709；公钥 `status-d-netrpi.txt:292`。低 |
| 12 | devyanwei | `4QFT0wob/9iYuqTbo1GHirj8ZgdE85fTJh1nf7JqgCw=` | `sha-g0FFOR5DUX4tAfCfE/+/Ewh2ozDq+G0fdQySHQr+xM0=` | 同左 | ✅ | [B] netrpi.log:801→802；公钥 `status-d-netrpi.txt:68`。低 |
| 13 | admindingzuwei | `x7LsrNP8NRtPNL93Lk0wwloF+XW9qsYTZdkzjEJp5Ck=` | `sha-z89n+zdU4/DMexgsKr8KVhbVo6WzhQl3JI+oOIfcT5A=` | 同左 | ✅ | [B] netrpi.log:873→874；公钥 `status-d-netrpi.txt:132`。低 |

原文摘录（file:line）：
- `netbird-debug-netrpi.log:137`：`…INFO …client.go:245: create new relay connection: local peerID: F0GVt4oEj2ZGaHNqXjcotHHlsz3qhBHBm1sfou+e0DQ=, local peer hashedID: sha-27MDvCryE04wbNBnCifkvLIOAnBqfhhM42r/DiK7Cp0=`
- `netbird-debug-netcenter.log:95`：`…INFO …client.go:254: create new relay connection: local peerID: ftUD3aGWs/FmwtK3D44fnZAQatiO+xSfW/1zdHAkdkI=, local peer hashedID: sha-LICqXrzbd23hUhgZ48ehPylwY/UU3W9riI8MF7cjG+w=`
- `netbird-debug-netrpi.log:373→374`：`…DEBG …manager.go:201: open peer connection via permanent server: sNerYoiB…` → `…INFO …client.go:306: prepare the relayed connection, waiting for remote peer: sha-RoRvB5M…`
- `netbird-debug-netcenter.log:4`：`…client.go:801: remote peer has been disconnected, free up connection: sha-27MDvCryE04wbNBnCifkvLIOAnBqfhhM42r/DiK7Cp0=` —— netcenter 侧独立观测到 netrpi 的 ID，与 netrpi 自报一致（跨设备互证）。
- 接收侧语义佐证：`netbird-debug-netcenter.log:296` `buffered early transport message for peer: sha-lHWo…`（sha-lHWo = nethostdebian，发送方；符合服务端改写 `relay/server/peer.go:223` 与规格 §4.2）。

注：日志内嵌 `client.go:662/245/306`、`manager.go:201` 为 daemon 0.77.1 构建内行号；参考检出同消息位于 `client.go:659/254/306`、`manager.go:192`，文本相同、行号小幅漂移（见 §5）。

## §4 差异形态与候选修正

**不适用——未发现任何不一致**（13/13 逐字符一致；无长度、前缀、base64 vs raw、padding 或"对字符串还是对解码字节哈希"的差异可报），无需修正。

## §5 无法确证的点（残余不确定）

1. **`netbird-ohos`（我方设备对端）未被官方日志直接覆盖**：其公钥 `TrBfURPG8gHinPcgt2J9j1lsfe1EHwXI98U8WiG4iF4=`（`status-d-netrpi.txt:20`，采集期间 Status: Connecting）按公式算得 `sha-RGMnYC/RN2liZBDYQbxwpgAgJtD+1y6gRZc0YAvcsEo=`，但该值未在官方日志中出现——公式由其余 13 键实证，对该具体键是外推（待确认，可在我方设备连上同一 relay 后用官方对端日志一次确认）。
2. **版本漂移**：实测 daemon 0.77.1，参考检出为 `791401060d2b`；日志内嵌行号与参考检出存在小幅漂移（同消息文本）。13/13 吻合是实测层面的结论，不依赖参考检出与运行版本完全同源。
3. **本验证覆盖"推导"，不直接覆盖"线上编码"**：日志给的是可读形（`"sha-"+base64(hash)`）；线上 36B 原始字节形态由 `id.go:20-22` 与 golden 测试（`relay.rs:627` 线上 hex `7368612d…`）锚定，本次未解析 pcap 复核线上帧字节（如需可对 `oracle-relayed*.pcap` 做 dstID 字段抽样）。
4. **时间线配对（[B] 类 11 例）依赖同日志毫秒级相邻**，非单行直证；代码路径唯一（`client.go:302→306`），且 19/19 命中，风险很低但非零。
5. **帧型覆盖**：证据来自 relay 会话的 Transport/订阅日志；Status 文件不含 sha- ID（仅公钥），HealthCheck/Auth 等帧型的 ID 用法未由本数据集独立验证。
6. 7 个采集期间离线/连接中的 peer（kihhayhz0001/0002、kihhrhhz0001、blkstation、devshenchaojue、hhdynenventry、netbird-ohos）无官方日志 ID 可比对（待其上线后补样）。
