# 首次开源发布检查

日期：2026-10-16。检查针对当前工作目录和独立发布副本，不代表真实平台或 Windows 完整验收。

## 内容

- [x] MIT 许可已确认，JS/Rust 元信息同步，第三方图标许可保留。
- [x] README、产品/架构/平台文档改为当前状态；历史开发文档本地归档。
- [x] 原 Git 历史保留，未提交浮窗排序源码/测试保留。
- [x] 按清单导出并逐文件校验 SHA-256；本地 Markdown 链接纳入导出前检查。
- [x] 发布清单排除 .git、数据库、认证文件、输出、旧图标候选与设计截图。
- [x] 自动检查结果已记录；Rust 完整测试存在下述失败项，未标全部通过。

## 发布前仍需

- [ ] 干净 Windows 目录安装锁定依赖，完整桌面启动与安装包实测。
- [ ] 真实账号与官方页面数值/重置对照，见 TASKS.md。
- [ ] 锁定依赖及传递依赖完整许可审计，二进制分发声明。
- [ ] 仓库归属/名称与私密漏洞报告设置。
- [ ] 在独立副本创建首次提交并推送 GitHub；当前不创建远程或公开发布。
- [ ] 新版真实桌面截图（脱敏）与首个 Release 安装包。

## 实际检查记录

本轮使用 Node.js 24.21.0、Rust/Cargo 1.95.0 和当前已安装依赖：

| 检查 | 结果 |
|---|---|
| 版本一致性 | 0.1.0，通过 |
| TypeScript、ESLint | 通过，含新增 Vitest 配置 |
| Node 版本脚本测试 | 1 项通过 |
| Vitest | 4 个测试文件、27 项通过 |
| Vite 前端构建 | 通过 |
| cargo fmt/check --locked/clippy --locked --all-targets | 通过；环境有增量缓存硬链接降级复制警告 |
| cargo test --locked --lib | 89 通过、1 失败、7 忽略 |
| 导出保护 | 已存在目标及 output/open-source 外目标被拒绝，未覆盖源目录 |

Rust 未通过项：providers::bailian_cli::tests::child_success_failure_timeout_and_output_limit，bailian_cli.rs:692，错误码 cli_query_failed。历史运行也有此失败，本轮不改变适配器，不跳过失败项来声称全绿。

修正了 Vitest 默认扫描误收集 Node 测试脚本及 output 副本的问题：独立 vitest.config.ts 将 include 限定于 src。参考 [Vitest include 文档](https://github.com/vitest-dev/vitest/blob/v5.0.3/docs/config/include.md)，通过 Context7 核对。

pnpm --version 无法启用 packageManager 指定版本，报 ERR_PNPM_LOCKFILE_WRITE_FILE 并回落到本机 12.8.1；没有改动系统权限或锁文件。前端检查直接调用 node_modules 内的已安装工具，未验证干净目录安装或常规 pnpm 入口。未启动 Windows 桌面、未构建新版 exe/安装包、未读取真实账号。

导出模式扫描覆盖常见 Key/Token/私钥及个人开发路径，不是完整秘密审计或依赖许可审计。SHA-256 只证明导出字节与本轮选定文件一致。

## 使用方式

原仓库执行 node scripts/prepare-open-source.mjs，生成 output/open-source/StillEnough/，清单置于副本旁。已有目标拒绝覆盖，后续重跑需指定新目录，见 [制作开源副本](OPEN_SOURCE.md)。只发布该副本内容，不上传整个开发目录或 output。
