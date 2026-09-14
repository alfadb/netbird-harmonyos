# 模拟器路线评估（2026-09-14）

> 结论：**当前环境下模拟器无法用于验证本项目的产品路径**（含壳侧烟测）。
> 两个阻断都是结构性的，与签名凭据无关。真机（物理冻结元组）仍是唯一可验证路径。

## 试过什么

1. 按"安装最新版模拟器"的要求，用模拟器自带 CLI 自助下载并安装最新镜像：

   ```
   Emulator -install -deviceType phone -osVersion "HarmonyOS 7.0.0(26.0.0)"
   ```

   成功：2,090,041,539 bytes → `~/harmonyos/emulator-images/system-image/HarmonyOS-7.0.0/phone_all_x86/`
   （`system.img` 3.67 GB）。该镜像是**最新可用**版本（`-imageList` 显示 7.0.0.106 / API 26，
   另有 6.1.1(24)、6.1.0(23)），且与真机同代 OS（真机为 7.0.0.105）。
2. `Emulator -create` 新建两个实例（`netbird_api24_phone`、`netbird_api26_phone`）并分别冷启动。
3. API 24 实例上实测安装产品 HAP；API 24 实例 `bm dump -a` 对比 VPN 组件存在性。

## 阻断 A：API 26（7.0.0.106）镜像在本机无法完成开机

两次独立冷启动（端口 10010）均在 guest uptime ~78–79 s 处由 pid=1 `init` **主动**触发：

```
sysrq: Trigger a crash
Kernel panic - not syncing: sysrq triggered crash
```

qemu 随即退出，端口不再恢复；**2/2 完全可复现（同一 uptime）**。panic 前的日志是 `DoUmount /vendor`、`/data`
的 init 重启序列，并有 `Create directory '/log/startup' failed err=30`。
日志：`~/harmonyos/emulator-instances/netbird_api26_phone/Log/kernel.log`。

**疑似原因**：本机 Emulator 二进制为 **26.0.0.400**，而 command-line-tools 为 26.0.0.821、镜像为
7.0.0.106 —— 7.0.0 镜像可能需要更新版 Emulator（DevEco Studio 自带版本）。

## 阻断 B：ABI 不匹配——产品 HAP 装不上任何本地模拟器

API 24 实例上 `hdc -t 127.0.0.1:10000 install <product HAP>` 返回：

```
error: failed to install bundle.
code:9568347 error: install parse native so failed.
In the module named entry, the Abi type supported by the device does not match the Abi type
configured in the C++ project.
```

原因：产品 HAP 只含 **arm64-v8a** 的 `libnetbird_core.so`，而**模拟器所有形态均为 x86_64**。
与签名、API 版本无关。因此 `CRED_WANT_OVERRIDE`、`Index` 页状态、`Connect VPN` 失败路径等
**壳侧观测同样无法在模拟器上取得**。

## 顺带确认的两点（对未来有用）

- **签名不是阻塞点**：模拟器接受 **unsigned HAP**（历史 `EV-E3-EMU24-20260717-0003/0004` 证据 +
  本次实测：安装流水线走到 native so 解析阶段才被拒）。若将来走模拟器路线，**不需要** AGC 的
  「模拟器调试凭据」。
- **VPN 组件缺失复现**：API 24 实例 `bm dump -a`（63 个 bundle）中 **零** VPN / vpndialog 命中，
  与 `EV-E3-API24-EMU-MATRIX-20260717-0001`「三种 Emulator 形态均缺 VPN 授权注册组件」一致。

## 若将来仍要走模拟器路线，前置条件是

1. **x86_64-ohos 的 core 构建**（`x86_64-unknown-linux-ohos`）或一个**无 native 库的纯壳侧 HAP 变体**；
2. **与目标镜像匹配的更新版 Emulator 二进制**（> 26.0.0.400，或 DevEco Studio 自带版本）；
3. 并且要接受：**即使两者都满足，模拟器仍无 VPN 授权组件**，数据面（VpnExtension / TUN / protect）
   在该形态下不可执行 —— 这正是项目把 N1b 定为物理门的原因。

## 环境残留（未删除，供后续决定）

- `~/harmonyos/emulator-instances/netbird_api24_phone{,.ini}`、`netbird_api26_phone{,.ini}`
- API 26 镜像目录 `~/harmonyos/emulator-images/system-image/HarmonyOS-7.0.0/`（约 2.09 GB 归档 + 解包镜像）
- 两个实例均已 `emulator-stop`，零残留进程/端口；真机 `192.168.50.199:37193` 全程未被本评估触碰。
