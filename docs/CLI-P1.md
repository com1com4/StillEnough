# CLI P1 实施与验证

日期：2026-10-18。P1 已实现；全仓库测试仍存在下述失败，不标记整体验收全通过。P2–P5 未实施。原设计提案 `CLI-DESIGN.md` 保留，本文是其 P1 落地记录。

## 已实现

- 根 Cargo workspace：桌面 host、无 Tauri 的 stillenough-core、独立 stillenough-cli。唯一 Cargo.lock 移到根，原有外部依赖版本全部保留，仅新增两个本地包；release profile 移到 workspace 根。
- ProviderDescriptor 和静态描述集中到 core，原桌面适配器委托同一目录。CLI 仅输出六字段白名单投影，保持所有八个 Provider（包括旧连接 codex），不改变实测声明。桌面注册表一致性测试防止目录与适配器脱节。
- version/providers：制表符表格、UTF-8 JSON envelope、Provider 筛选，未知命令/参数退出 2；固定错误文案不回显参数。--timeout 1–300（默认语义 30 秒，本地命令立即完成，无等待任务）、--no-color；输出始终不使用 ANSI。--help/--version 为纯文本。
- core 定义响应 DTO、账号 status 与指标 freshness 独立枚举、百分比未知/零及钱包无百分比构造规则。错误映射、TTL/重置计算与账号投影仍留待 P2；本阶段不声称已统一运行时状态计算。
- 不接数据库、凭据、IPC、网络、授权设置、调度或刷新。CLI 无 Tauri/build.rs，不需要前端产物。schemaVersion=1、protocolVersion=1.0 与产品版本分别维护。
- 版本同步脚本/测试与开源复制脚本适配根锁文件及 crates；源码运行直接使用 Cargo，分发 scripts 留待 P5。

## 运行

```powershell
cargo run -p stillenough-cli --locked -- version --json
cargo run -p stillenough-cli --locked -- providers
cargo run -p stillenough-cli --locked -- providers --provider codex_http --json
cargo build -p stillenough-cli --locked
& ./target/debug/stillenough-cli.exe providers --json
```

桌面仍使用 `pnpm tauri dev`。构建输出根目录变为 `target/`；原 `--manifest-path src-tauri/Cargo.toml --locked` 仍可用，但全量检查应使用 --workspace。

## 实际检查

环境：Windows，Node 24.21.0，Cargo 1.95.0。初始工作区仅有未提交 docs/CLI-DESIGN.md，未覆盖其内容；本阶段未提交。

| 检查 | 实际结果 |
|---|---|
| 改动前 pnpm test | ERR_PNPM_LOCKFILE_WRITE_FILE（权限）；lint/typecheck 在沙箱内同样受阻 |
| 改动前 Rust 全套 | 91 通过、2 失败、7 忽略；百炼 child_success_failure_timeout_and_output_limit，以及代理 system_environment_priority_and_bypass_are_isolated_in_child（WouldBlock/IncompleteMessage） |
| pnpm test / pnpm lint / pnpm typecheck | 沙箱外且 TEMP/TMP 指向工作区 output/qa-temp 后全部通过；版本脚本 1 项、Vitest 5 文件/36 项。默认沙箱直接执行仍有权限限制；中途直接调用已安装 Vitest 遇到 Temp 缓存 ENOENT，不能代替本行结果 |
| cargo fmt --all -- --check | 通过 |
| cargo check --workspace --locked | 通过 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 通过；最初新增代码冗余字段写法已修正后复验 |
| cargo test --workspace --locked | 最终桌面 92 通过、2 失败、7 忽略（含新增注册表一致性测试）；百炼历史子进程用例 cli_query_failed 和基线代理子进程 WouldBlock 再次失败；中间一次为 92 通过、1 失败、7 忽略；此命令因桌面失败提前返回，不据此声称新增 crates 已运行 |
| cargo test -p stillenough-core -p stillenough-cli --locked | core 2 项和 CLI 进程契约 1 项通过；覆盖多个退出码/参数、Unicode、stdout/stderr 秘密哨兵 |
| JSON Schema 2020-12 | 从设计提案完整提取 schema，使用 jsonschema 4.26.0 + FormatChecker 验证 version/providers/筛选/错误响应；通过 |
| Windows CLI smoke | PowerShell 7 的真实 PTY 中文输出、PowerShell 5.1 调用中文输出、PS7 文件重定向通过；UTF-8 无 BOM、JSON 可解析、无 CLI ANSI。PTY 工具自身控制码不属于程序 stdout |
| 桌面启动 smoke | cargo build -p stillenough --locked 通过；Vite 1420 启动后运行新 target/debug/stillenough.exe，6 秒后进程存活，原生主窗口句柄非零、标题 StillEnough。未以浏览器预览替代原生启动 |

Schema 用例仅使用测试依赖（安装到忽略的 output/qa-deps，不是运行依赖）：

```powershell
New-Item -ItemType Directory -Force output/qa-temp | Out-Null
$env:TEMP = (Resolve-Path output/qa-temp).Path
$env:TMP = $env:TEMP
python -m pip install --target output/qa-deps jsonschema==4.26.0
cargo build -p stillenough-cli --locked
python scripts/check-cli-schema.py
```

秘密哨兵覆盖 P1 实际输入边界：参数、Provider 筛选、环境中的 API Key/认证目录/代理，断言 stdout/stderr 不含哨兵；CLI 不读这些环境来源。没有 header/body/厂商 stderr/数据库/导出路径，不能把本阶段测试说成已覆盖后续后台泄露边界。

## 未验证与下一步

未做人工视觉/交互验收、干净 Windows 安装、PowerShell 5.1 文件重定向、安装包与真实账号查询；原生启动仅证明进程及窗口创建。pnpm tauri dev 的组合启动命令未单独验收（原生 smoke 为 Vite + Cargo 构建产物）。百炼历史用例与代理基线波动保留，不为通过检查改写无关业务。

下一阶段 P2 才增加只读离线仓储、UI Agent 授权与 freshness 计算；不得提前接入凭据、网络刷新或 IPC。
