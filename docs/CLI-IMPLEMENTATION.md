# CLI 实施与验证记录

日期：2026-10-18。本轮在未提交 P1 基础上继续实现 P2–P5 的代码；版本仍为本地开发构建 0.2.0，不代表公开 Release 或 npm 已发布。原设计与 P1 检查记录保留，本文记录后续落地。

## 实现

- P2：独立 agent_access（默认全 false、缺失/损坏/未知版本/无有效更新时间拒绝），main-only 设置 UI/命令；固定用户数据目录的 query_only/READ_ONLY 事务；schema 1–6 完整性门槛；账号清单、quota、doctor 离线读取。金额精度、0/null、钱包无百分比、独立 freshness 保持契约；Rust 与前端共用边界 fixture。
- P3：Windows 命名管道，SID/会话/规范化目录派生端点，显式 DACL + 拒绝远程，双向进程 SID/会话/提权校验，长度限制、请求截止时间、协议/schema 版本和 instanceId/revision。只有 endpoint 不存在才回退已授权只读缓存。跨会话 Global 目录所有者内核对象在开库前获取，持有到退出，阻止同用户跨会话第二 writer。
- writer：WAL、5 秒 busy_timeout、启动 quick_check、未来 schema 拒绝、旧 schema 迁移前 VACUUM INTO 一致性备份；不变更 001–006 序列。CLI 不创建/迁移/写库、不读取认证文件或 keyring，不直接调用 HTTP/厂商 CLI。
- P4：现有刷新内核改为显式 state、权限 guard、事件回调；Tauri 与管道调用同一 Runtime/gates/semaphore/持久化/提醒。不为模块搬迁复制 Provider，也不在 CLI 启动后台。队列前后重查权限、账号启用、连接 revision、来源授权和平台冷却；刷新错误码白名单保存，结果发送前再查授权。全局暂停只阻止自动采集；显式单次刷新遵守防抖和平台限流。
- operation：请求 ID 去重，有界实例内结果表，超时返回 operationId，不声称撤销；operations get 不重发网络。刷新批量部分失败退出 8，错误优先级固定；doctor 默认不读凭据，live 走同一刷新服务。
- P5：源码脚本、cargo install、Windows CLI ZIP、包含桌面/CLI 的 ZIP、独立 npm wrapper/平台包打包、NSIS sidecar 构建入口、SHA256 清单、Agent 使用文档。没有发布公共包或修改全局 PATH。

所有真实数据仍来自用户在桌面配置的连接，无演示模式、示例账号或模拟值回退；合成输入只存在自动测试中。

## 实际检查

环境：本机 Windows，Node 24.21.0、Cargo/Rust 1.95.0，packageManager 固定 pnpm 11.9.0。检查输出在忽略的 output/cli-*.log；持久化结论以本文为准。

| 检查 | 结果 |
|---|---|
| pnpm test/lint/typecheck/build | 普通用户（沙箱外）且 TEMP/TMP 指向 output/qa-temp 后通过；版本脚本 1 项，Vitest 6 文件/42 项 |
| cargo fmt --all -- --check | 通过 |
| cargo check --workspace --locked | 通过 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 通过；Windows 沙箱内有增量缓存硬链接降级提示，命令退出 0 |
| cargo test --workspace --locked | 普通 Windows 用户环境完整运行通过；最终计数与补充检查见下方收尾记录。7 项真实凭据/真实账号/开机启动专项仍忽略 |
| JSON Schema 2020-12 | version/providers/筛选/参数错误，以及合成 accounts/snapshot/diagnostics 通过 FormatChecker；UTF-8 无 BOM、单对象和秘密哨兵通过 |
| Windows pipe | 真正 OS 管道测试通过；并发快照、身份检查、首实例拒绝、协议不匹配、授权撤销，以及超长/截断帧测试通过 |
| 刷新服务 | 合成传输下的全局并发 3、批量部分失败、防抖、去重、撤销排队、截止时间后操作完成/丢失通过；不是新真实账号实测 |
| cargo install --path ./crates/stillenough-cli --locked --root ./output/cli-install | 实际安装成功；未改全局 PATH。保留原锁解析，registry 曾提示原锁中的 yoke-derive 0.8.3 被 yanked，未擅自更新依赖 |

### 基线与环境差异

P1/改动前在沙箱内曾复现百炼 child_success_failure_timeout_and_output_limit 和代理 system_environment_priority_and_bypass_are_isolated_in_child 失败，本轮沙箱内仍有同类限制；不能抹去历史失败。普通 Windows 用户下完整测试中两个既有用例均通过；这证明本轮环境下的结果，不宣称所有环境都已稳定。

严格 DACL 的 pipe 在沙箱 AppContainer 中连接被拒绝；普通用户进程验证通过，未为沙箱加入额外 DACL 放行。新增 Tauri MockRuntime 测试初遇 comctl32 入口装载错误，已通过仅测试链接的独立 manifest 资源修正；没有依赖外部 exe.manifest 或全局 Windows 设置。

测试哨兵注入参数、环境、凭据引用、连接路径及错误来源，白名单投影不透传；P4 传输替身只在 cfg(test) 编译，产品中不存在替身或凭据绕过分支。

## 验证边界

不把自动化 MockRuntime 或浏览器构建称为 Windows UI 人工验收。跨用户/提权/远程机器/跨会话对抗测试、磁盘满与断电迁移中断、干净 Windows/无 Node/无 WebView2 机器、完整 NSIS 安装卸载与签名、远端 CI（本次未配置）、公共 npm 包名归属/发布仍需对应环境。没有申请签名证书、发布账号或新增付费服务。

core 已承载 CLI 共用契约、只读仓储、freshness 和传输；Provider 网络/凭据/调度仍在唯一桌面 host。刷新核心已经去除 AppHandle 依赖，入口适配器保留 AppHandle。这一阶段没有为追求目录搬迁把桌面依赖反向引入 CLI；依赖树确认 CLI 不含 Tauri、WebView、keyring 或 reqwest。

## 收尾记录

- 完整工作区复核：桌面 105 通过、7 忽略，core 16 通过，CLI 进程契约 1 通过；合计 122 通过，0 失败。7 项专项未运行，不属于全功能 Windows 验收。
- 实际生成独立 ZIP 和两个本地 npm tgz；离线 npm install --ignore-scripts 安装成功，wrapper version JSON 退出 0、未知参数 JSON 退出 2。中文/空格路径及 .cmd 调用通过。
- PowerShell 7 直接 UTF-8 重定向、PowerShell 5.1 显式 UTF-8 捕获/写文件均通过；未声称 PS5 默认 > 会保留 UTF-8。
- dumpbin 确认独立 CLI 不含 WebView/Tauri 依赖，但仍依赖 VCRUNTIME140.dll；无该运行库的干净系统未验证。
- PowerShell 5.1 打包脚本初遇 UTF-8 无 BOM 中文解析错误，已改用 UTF-8 BOM；npm 默认缓存权限失败已通过工作区专用离线缓存修正。失败的 output/cli-validation 目录保留，未覆盖或当作最终产物。

- 原生桌面冒烟通过：实际窗口标题 StillEnough、句柄非零，CLI quota 通过 ipc 收到默认 agent_access_denied/退出 4；没有开启 Agent 权限或主动刷新真实账号。初次冒烟发现主线程缺 Tokio reactor，已修正并增加同步 setup 绑定回归测试；未以 MockRuntime 通过冒充原生验证。
- pnpm cli -- version --json 的 stdout 实际解析通过；Cargo/pnpm 进度在 stderr。

- 最终 release 构建与打包目录冒烟通过（不是旧二进制）：窗口 StillEnough、CLI transport=ipc、默认拒绝退出 4。最终 ZIP 两份、npm tgz 两份、NSIS 一份位于 output/cli-release/v0.2.0，SHA256SUMS.txt 已逐项核对。
- NSIS 已通过已构建 release 的 tauri bundle --bundles nsis --config … --ci 生成；生成的 installer.nsi 明确包含 /oname=stillenough-cli.exe。仅生成，未运行安装器、未改注册表或覆盖当前安装。
