# 独立 DNS probe

仅依赖 POSIX / libc，不接入产品构建。用同一目标名对比系统 `getaddrinfo` 与指定 IPv4 DNS 服务器的 UDP / TCP 查询。工具自身不读系统配置文件；`getaddrinfo` 的平台实现仍可能读取其正常配置。显式模式发送一条 IN / RD=1 查询，不迭代、不追踪 CNAME、不自动从 UDP 切 TCP，不绑定本地 53 端口。

```sh
./dns-probe --system example.com
./dns-probe --server 100.64.0.1 --udp --type A example.com
./dns-probe --server 100.64.0.1 --tcp --type A example.com
./dns-probe --server 100.64.0.1 --udp --type AAAA example.com
./dns-probe --server 100.64.0.1 --tcp --type AAAA example.com
```

将示例服务器替换为本轮验收指定服务器。默认目的端口 53，可用 `--port 1053` 指定测试端口；默认整次操作超时 3000 ms，可用 `--timeout-ms 1..60000` 修改。显式模式用非阻塞 socket 和统一单调时钟截止时间；进程级 `setitimer` 也限制系统解析及其他阶段，超时输出固定消息并退出 3。系统模式输出 `getaddrinfo` 数值返回码、文字错误，以及 `EAI_SYSTEM` 对应 errno 和 A/AAAA 地址。超时强制结束时不会伪造 getaddrinfo 返回码。

输入仅支持 ASCII 主机名（IDN 请先用 punycode），标签 1..63 字节，去除一个可选尾点后最长 253 字节、wire 最长 255 字节；拒绝空标签、非法字符和标签首尾连字符。显式查询校验服务器连接、ID、QR/opcode、唯一 question 的 name/type/class、记录及压缩名边界。UDP 忽略不同 ID 的包且不延长截止时间；同 ID 的错误 question 返回 4。答案摘要显示 name/type/class/TTL/rdlength，A/AAAA 显示地址，CNAME/NS/PTR 显示名称，其余记录只显示元信息。未知字节不按字符串输出，名称特殊字节以十进制转义。压缩指针仅接受向后引用并限制展开长度与跳转次数。

退出码：

| 码 | 含义 |
|---|---|
| 0 | 系统解析成功，或收到校验通过的 DNS 响应；任何 rcode（包括明确显示的 `3(NXDOMAIN)`）均属于有效响应 |
| 2 | 参数或主机名不合法 |
| 3 | 传输失败、TCP 提前 EOF、超时或本地运行错误 |
| 4 | DNS 帧、关联 question 或记录格式不合法 |
| 5 | getaddrinfo 返回错误（数值错误见输出） |

`TC=1` 的结构完整响应返回 0，但不能当作完整答案；应另跑同名 TCP 查询。若记录实际缺字节，返回 4；TCP 声明长度未收齐即关闭返回 3。返回 0 不等于域名存在或业务可达。

## 手工构建（输出文件必须尚不存在）

以下参数沿用 client/core/build.sh 的 SDK 根目录与 OHOS target；不运行产品构建脚本。

```sh
src=/home/worker/n2h-kit/wt-native-dns/client/tools/dns_probe.c
out=/home/worker/n2h-kit/native-dns-probe
sdk=/home/worker/harmonyos/command-line-tools/current/sdk/default/openharmony/native
# 仅在输出目录不存在时创建；已有目录先检查内容，避免覆盖。
# mkdir "$out"
test ! -e "$out/dns-probe-host" &&
cc -std=c11 -O2 -Wall -Wextra -Werror "$src" -o "$out/dns-probe-host"
test ! -e "$out/dns-probe-aarch64" &&
"$sdk/llvm/bin/clang" --target=aarch64-unknown-linux-ohos --sysroot="$sdk/sysroot" -std=c11 -O2 -Wall -Wextra -Werror "$src" -o "$out/dns-probe-aarch64"
test ! -e "$out/dns-probe-x86_64" &&
"$sdk/llvm/bin/clang" --target=x86_64-unknown-linux-ohos --sysroot="$sdk/sysroot" -std=c11 -O2 -Wall -Wextra -Werror "$src" -o "$out/dns-probe-x86_64"
file "$out/dns-probe-host" "$out/dns-probe-aarch64" "$out/dns-probe-x86_64"
```

本轮主机回环测试通过：UDP/TCP 的 A、AAAA、NXDOMAIN、TC、150 ms 超时、压缩 owner/CNAME；TCP 按单字节分段发送的长度头与正文、截断正文/长度头；压缩环/越界指针和错误 question 拒绝；UDP 不同 ID 后接正确响应；非法主机名拒绝、253 字节主机名接受。主机 `--system localhost` 输出 `AAAA ::1` 和 `A 127.0.0.1`。主机及两种 OHOS 构建均通过 `-Wall -Wextra -Werror`。`file` 确认为 AArch64 / x86-64 ELF64 PIE，OHOS interpreter 分别为 `/lib/ld-musl-aarch64.so.1` 与 `/lib/ld-musl-x86_64.so.1`。仅进行了本机构建和回环测试，未在设备执行。

## 后续设备侧手工使用（本轮未执行）

确认设备架构后选对应 ELF，先确认设备目标路径无同名已有文件，再传入：

```sh
hdc file send /home/worker/n2h-kit/native-dns-probe/dns-probe-aarch64 /data/local/tmp/dns-probe
hdc shell chmod 700 /data/local/tmp/dns-probe
hdc shell /data/local/tmp/dns-probe --system example.com
hdc shell /data/local/tmp/dns-probe --server 100.64.0.1 --udp --type A example.com
hdc shell /data/local/tmp/dns-probe --server 100.64.0.1 --tcp --type A example.com
hdc shell /data/local/tmp/dns-probe --server 100.64.0.1 --udp --type AAAA example.com
hdc shell /data/local/tmp/dns-probe --server 100.64.0.1 --tcp --type AAAA example.com
```

每次查询前后记录客户端 DNS 计数及工具输出/退出码。shell UID 的系统解析路径、缓存和网络权限可能与 App 系统解析不同；shell 工具结果只能作为辅助比较。最终仍以 TUN 到达证据和浏览器实际行为为准；TUN 计数本身不含 UID，不能单独归因到某个 App。
