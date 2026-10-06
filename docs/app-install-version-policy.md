# App 安装版本策略（HarmonyOS 客户端）

适用范围：`client/` 产品 HAP 的每一次真机部署。本文只约定规则，不含自动化脚本，也不要求构建时自动递增版本号。

## 权威配置

- `client/build-profile.json5` 中 `app.products[name=default]` 的 `versionCode` / `versionName` 是**权威值**。
- `client/AppScope/app.json5` 的 `versionCode` / `versionName` 必须与权威值一致；两处不一致视为配置漂移，先修再打包。
- 基线：versionCode 1 / versionName 0.0.1 已安装。下一次新部署：versionCode 2 / versionName 0.0.2。

## 每次新部署前

1. `versionCode` 必须同时**大于设备当前已安装值**和**上一个已发布/已部署版本**；`versionName` 的 patch 位递增（0.0.1 → 0.0.2 → …）。
2. 同步修改上述两个配置文件，两处取值必须一致。
3. 打包后由 `client/build.sh` 的 pack.info 校验步骤核对：HAP 内 pack.info 的 version code/name 与两处配置一致，否则构建失败（校验用 python3 的 json5 真正解析 JSON5，不做文本假装解析）。

## 失败重试与产物复用

- 传输或安装失败、且**产物未在设备上装上**时，允许用**同一个未安装成功的产物**直接重试，不必改版本号。
- 已成功安装过的产物不得再用于后续部署；重装相同 versionCode/versionName 的同一产物不算新版本部署。
- 只要某个产物已在设备上安装成功，下一次部署就必须使用更大的 versionCode 和递增的 versionName。

## 安装后核验

```bash
hdc shell "bm dump -n cn.alfadb.netbird"
```

读取输出中的 `versionCode` / `versionName`，确认与配置一致，并等于设备实际安装的版本；UI「设置 → 关于」显示安装包实际的 versionName 与 versionCode（取自 `BundleInfo`），可交叉核对。
