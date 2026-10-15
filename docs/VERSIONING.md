# 版本管理

`package.json` 的 `version` 是应用版本唯一编辑入口，当前为 `0.1.0`。侧栏和关于页在前端构建时读取它；Tauri 配置的 `version` 引用 `../package.json`，用于桌面应用与安装包版本。Rust 包版本与 Cargo.lock 中本应用的版本通过脚本同步。

## 升级版本

在项目根目录运行以下任意一条：

```powershell
pnpm version:set patch   # 修复：0.1.0 → 0.1.1
pnpm version:set minor   # 功能：0.1.0 → 0.2.0
pnpm version:set major   # 不兼容变更：0.1.0 → 1.0.0
pnpm version:set 0.2.0   # 指定版本
pnpm version:check
```

目前仅支持三段非负整数版本，不提供预发布后缀。脚本同步 package.json、src-tauri/Cargo.toml 和 Cargo.lock 的应用条目，不修改第三方依赖，不创建 Git 提交或标签。

`pnpm build` 会先检查版本一致性。`pnpm test` 包含临时目录中的版本脚本测试，覆盖三种升级、指定版本、非法输入和漂移检测/修复，不更改仓库版本。

如本机 pnpm 入口受权限限制，可直接运行 `node scripts/version.mjs patch`、`node scripts/version.mjs --check` 与 `node --test scripts/version.test.mjs`。直接调用 Vite 构建时也应先运行版本检查。

## 发布记录与构建

升级时同步记录 TASKS.md 中的已实现功能、实际检查与待验证项，并提交上述三个版本文件。版本号不表示适配器已通过真实账号验证。

修改版本后须重新构建桌面程序，例如 `pnpm tauri build --no-bundle`；仅修改源码或运行 `pnpm build` 不会更新已有 exe。关于页显示当前前端资源编译时的版本。

Tauri 版本字段参考：[官方配置说明](https://v2.tauri.app/reference/config/#version)。
