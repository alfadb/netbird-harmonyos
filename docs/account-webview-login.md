# 账户网页登录

状态：功能已接入并完成 0.0.7 构建、签名及离线验证。Setup Key 登录仍保留。

## 使用方式

安装应用后，先断开当前 VPN，再进入「设置」→「账户登录」，在应用内打开的页面完成账户登录。成功后应用会自动连接 VPN。登录过程中取消会安全退出，不会改变此前选定的登录模式。账户登录与 Setup Key 是两种登录模式，切换前须断开 VPN。

网页登录使用应用内 WebView，不跳转外部浏览器；应用不读取或记录用户输入的密码，也不向网页提供 JavaScript 桥接。

## 登录与令牌

- 使用 OAuth 授权码流程和 PKCE S256；校验服务器提供的精确回调 URI 与会话 `state`，拒绝隐式流程令牌。
- 授权端点和令牌交换均通过 HTTPS。服务端配置的 loopback 回调可能是 `http://localhost:53000/` 或 `http://localhost:54000/`；客户端遵循本次发现的精确 URI，不固定端口。
- 当前公开配置为 `https://api.netcenter.alfadb.cn/oauth2/auth` 和 `https://api.netcenter.alfadb.cn/oauth2/token`。未请求 `offline_access`；连接使用 `access_token`，不以 `id_token` 替代。令牌到期时提示用户重新登录，不自动刷新。
- 令牌存入应用私有文件前使用 AES-256-GCM 加密；随机 32 字节密钥保存在 AssetStoreKit。未实现从 JWT 显示用户名的功能。

## 验证记录

- ArkTS 编译退出码为 0；生成的 `modules.abc` 包含 `AuthCore`、`Login` 和 `WebView` 模块。
- Rust 完整测试：870 项通过、0 项失败，覆盖 47 组（报告注明 518 个 lib 测试）。首次尝试因链接器路径缺失而未进入测试；修复后完整运行通过，无断言失败。报告：`/home/worker/n2h-kit/app-builds/account-v007-native-tests-20261006T124757Z/REPORT.md`。
- 入口测试：35 个 `run-*.mts` 脚本共 1,967 项通过；另一个独立 `run.mts` 有 81 项通过，共 36 个脚本、2,048 项检查。报告：`/home/worker/n2h-kit/app-builds/account-v007-entry-tests-20261006T125055Z/REPORT.md`、`/home/worker/n2h-kit/app-builds/account-v007-entry-base-20261006T125245Z/REPORT.txt`。
- 已执行只读实时服务端 metadata 发现，使用随包 CA；该操作未获取认证码，也未执行账户登录。记录：`/home/worker/n2h-kit/app-builds/account-v007-check-20261006T124350Z/native-live-discovery.txt`。
- 0.0.7 HAP 构建及签名验证均成功。签名包大小 5,499,792 字节，SHA-256：`c4efad019fc5a5af37e1ebbe4df16fe22e2565f5762ba60e9143acb6d0f69f0b`。清单：`/home/worker/n2h-kit/app-builds/account-v007-20261006T125102Z/MANIFEST.txt`。

## 尚未验证

签名 HAP 未安装到设备（phone006）；未在真机完成真实用户 SSO、令牌交换、AssetStoreKit 读写或 WebView 回调流程。因此构建和测试通过不代表真实账户登录已端到端验证。Setup Key 数据未迁移或改写，既有诊断信息保留。
