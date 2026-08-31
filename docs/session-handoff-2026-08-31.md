# 会话交接文档（2026-08-31）

最后核验：2026-08-31 ｜ HEAD `b842fe9` ｜ 工作树 clean、与 origin/main 同步

## 项目当前状态总览

netbird-harmonyos 项目在 HarmonyOS 上做 NetBird 行为兼容原生客户端。路线 A（ArkTS 壳 + C++ NAPI 薄桥 + 单一 native core）已由 T0 治理决议（`ADJ-T0-NATIVE-NX-20260830-0001`）确定为唯一可行性路线（B/E 方向因 phone 无公开进程启动机制已关闭）。

## 已完成的门

| 门 | 状态 | 证据 |
| --- | --- | --- |
| E0/E1-C/E2（Emulator 普通应用/C 网络） | reviewed-pass/pass | 2026-07-17 系列 |
| E3（模拟器 phone/2in1/Tablet） | reviewed-pass/blocked（镜像缺 VPN 组件） | 矩阵证据 |
| N0（BoringTun 0.7.1 C ABI Emulator 冒烟） | reviewed-pass/pass | `EV-N0-EMU24-20260810-0002` |
| E3-PHYS-PREFLIGHT（物理 VPN fd 可达性 S1-S7） | reviewed-pass/pass（consumed-pass） | `EV-E3-PHYS1API26-20260829-0001` |
| G0（stock Go arm64 loader 物理探针） | reviewed-pass/blocked（consumed-blocked） | `EV-G0PHYS1API26-20260830-0001` |
| **N1a（native WG 数据面 × 回环泵 Emulator）** | **reviewed-pass/pass** | `EV-N1A-EMU24-20260831-0008` |
| E1（stock Go loader） | reviewed-pass/blocked（dormant） | 三重实测证明不可达 |
| E8 | CLOSED | 开放前提已替换为 N6 |

## 正在进行：N1b（物理 VpnExtension fd 集成）

**状态**：判据 r0 已预注册（`docs/n1b-gate-plan.md`），独立审查已派发但**在出结果前被用户终止**（会话收尾，停止所有任务）。

**下一步**：

1. 重新派发 N1b 判据审查（审查者提示词见 `docs/n1b-gate-plan.md` 的审查要点段落；核心问题：tun fd 数据路径在 BoringTun ffi 上的正确衔接模式、fd 合同 FD-1..FD-7 完备性、探针在 VpnExtensionAbility 内的可行性）
2. 审查通过后冻结判据 -> 实现 -> host 验证 -> **物理 campaign 须用户显式授权**（pre-E8 native 例外，独立 AUTH/pair，沿 G0 13 门范式）
3. campaign -> 证据登记 -> 记录级审查 -> N1b 收口

## 关键治理文档（新会话必读）

| 文档 | 内容 |
| --- | --- |
| `docs/native-nx-governance.md` | T0 治理决议全文（路线 A、N1-Nx 七门、E8 前提替换、pre-E8 物理例外、fd 合同绑定条款 §二.2） |
| `docs/n0-native-client-feasibility.md` | N0 决议（方向 B、停止条件、compat oracle） |
| `docs/n1a-gate-plan.md` | N1a frozen-r3 判据（已收敛；C7 探针自有资源门、C5 三态） |
| `docs/n1b-gate-plan.md` | N1b r0 判据（待审查） |
| `docs/g0-go-arm64-physical-probe.md` | G0 计划（consumed-blocked 完结） |

## 冻结的物理元组

HarmonyOS / PLA-AL10 / `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` / API 26 / aarch64 / arm64-v8a

- 物理设备别名 `PHYS-1`，连接经 Windows 主机 Alfadb-V-Win（无线调试）
- 本 Linux worker 是**执行宿主**（E3 收官与 G0 均在此执行）；Windows 只承担签名构建
- 签名链完整（本机 hap-sign-tool + E3 连续性 .p12/cer + AGC App ID `cn.alfadb.netbird.g0probe`）

## 关键技术事实

| 事实 | 来源 |
| --- | --- |
| stock Go c-shared（1.25-1.27 全版本）IE TLS 形态不变，musl loader 拒绝 | E1 + G0 + Go1.27 研究 |
| phone 无公开进程启动+fd 传递机制（fork 禁止） | 两席交叉取证 |
| BoringTun 0.7.1 ffi 数据面 API：new_tunnel / wireguard_write / wireguard_read / wireguard_tick / wireguard_force_handshake / wireguard_stats / tunnel_free | crate 源码核实 |
| N1a Emulator 吞吐 55-105 MiB/s（地板 5 的 11-21 倍），双隧道回环全过 | N1a campaign 0004-0009 |
| VpnExtension fd：E3 实测 fd=33 创建、destroy 唯一关闭、进程边界 terminal | E3 S1-S7 |
| ArkTS Record 索引怪癖（cast 后方括号返回 undefined） | N1a 0007 缺陷 #8 |
| NAPI argc 是双向参数（输入=容量） | N1a 0009 process_model 修复 |

## 并行事项

| 项 | 状态 |
| --- | --- |
| N3 法律评估（shared/ BSD-3 声明效力 + combined/ 差异 + 生成代码义务） | 简报已备 `/tmp/n3-legal-brief.md`，**须用户提交外部法律意见**；N3 硬前置 |
| C5_PHASEB_FINAL_RULING | ✅ 已终裁 measured-fact、已登记 |
| process_model NAPI argc 修复 | ✅ 已修、下轮生效 |
| Go 1.27 研究 | ✅ RS-G0-GO127ELF-20260830-0001 已登记 |

## 后续路线图（按治理决议 N1-Nx 骨架）

```text
N1a ✅ -> N1b（当前）-> N2a/N2b（protect/路由壳）-> N3（management gRPC，法律前置）-> N4（signal+relay）-> N5（ICE）-> N6（端到端）-> N7（SLO）
```

## 新会话操作提示

1. `git pull` 确认 HEAD `b842fe9`
2. N1b 判据审查：重新派发（参照本文件"正在进行"段的审查要点）或检查是否有遗留结果
3. N1b 实现须等判据冻结
4. 物理 campaign 须用户显式授权新 AUTH/pair
5. N3 法律评估须用户操作（非 agent 可完成）
