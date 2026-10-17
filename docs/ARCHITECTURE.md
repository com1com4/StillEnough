# 当前架构

更新：2026-10-16。本页描述当前源码结构，以源码类型、迁移与测试为准。

## 分层

Tauri 2 承载主窗口、浮窗、托盘与菜单；React/TypeScript 负责交互，Rust 统一处理 Provider、凭据、网络、调度和 SQLite。没有远程数据库或常驻 Web 服务。

| 路径 | 职责 |
|---|---|
| src/App.tsx、src/components/ | 总览、连接、历史、设置、关于页 |
| src/widget/ | 浮窗 |
| src/lib/ | IPC、类型、主题、格式化、历史与排序 |
| src-tauri/src/commands/ | IPC、调用窗口检查、共享状态和事件 |
| src-tauri/src/domain/ | 模型、校验、币种代码 |
| src-tauri/src/providers/ | Provider trait、注册表、解析与查询 |
| src-tauri/src/services/ | 账号、连接、网络、调度、历史、告警、导出 |
| src-tauri/src/infra/ | SQLite、系统凭据与开机启动 |
| src-tauri/migrations/ | SQLite 版本迁移 |
| fixtures/providers/ | 自动测试合成响应 |

UI 不直接访问厂商、认证文件或 SQLite。主窗口与浮窗共享 AppState；变更增加单调 revision 并广播 state-changed，避免旧读取覆盖新状态。能力文件见 src-tauri/capabilities/，敏感管理命令另检查 main 窗口。

多语言展示集中在 src/lib/locales.ts、i18n.ts 和 providerPresentation.ts，以语义 key 和命名参数维护中英文资源，不新增运行时依赖。AppSettings.language 保存 system/zh-CN/en 偏好；resolvedLanguage 由 Rust 根据偏好与系统 UI 语言解析，忽略存储中的旧解析值。主窗口专用 set_language 命令先持久化再更新托盘并广播，两个 WebView 按后台设置重绘，Intl 使用所选语言和本地时区。旧语言字段异常仅回退该字段，保留其他偏好。

## 契约

前端类型见 src/lib/types.ts，Rust 类型见 domain/models.rs。AccountInput 只接收账号、产品、地址、启用、CLI 授权与网络配置，拒绝未知字段，不接受指标写入。

QuotaMetric 当前包含 metricId、label、kind、unit、remaining、limit、remainingPercent、resetsAt、source、observedAt、staleAfterSeconds。余额保留十进制文本，以 unit 保存币种或 currency_unknown；没有独立 amount/currency 字段。source 为 official_api 或 client_endpoint。

余额不生成百分比。额度仅按来源明确语义转换已用/剩余；缺失不等于 0/100%/不限量。失败保留真实观测原时间，重置到点不回填额度。DeepSeek/百炼使用共享离线币种校验表。

## Provider、凭据和网络

Provider trait 采用 boxed future，包含 descriptor、validate_url、fetch、fetch_routed、requires_credential、network_kind；集中注册，不在业务页面分散适配器分支。

API 凭据通过专用 IPC 进入短期后端会话，保存时写 Windows Credential Manager；SQLite 仅存不透明引用。CLI 来源须显式授权，百炼通过官方子进程管理登录，Codex/Claude HTTP 后台读取指定文件来源。不轮换 refresh token，不把令牌放进常规前端状态、SQLite、日志、fixture 或导出。

凭据服务 com.ai-plan-manager.api-keys 和历史数据目录保留兼容。Tauri 标识为 com.stillenough.desktop，通过 appDirectoriesOverride.localData 沿用旧目录，开源整理不改动它们。

统一网络支持 system/direct/custom，账号可 inherit；HTTP/SOCKS5/SOCKS5h 能力由适配器声明。不支持 PAC 或代理认证，失败不回退直连。CLI 子进程限制见 [平台说明](PROVIDERS.md)。

## SQLite

| 表 | 用途 |
|---|---|
| accounts | 产品、别名、启用状态 |
| connections | 地址、凭据引用、revision、重试时间、CLI 授权、网络 JSON |
| quota_metrics | 当前有效指标 |
| observations | 带时间的指标 JSON |
| fetch_states | 最近尝试/成功、状态与错误码 |
| app_settings | 设置 JSON |
| alert_state、alert_events | 阈值状态、周期和提醒去重 |
| credential_cleanup | 待清理凭据引用 |
| schema_migrations | 已应用版本 |

保留迁移 001–006 原序列，旧计费/人工模式由后续迁移清理；历史结构中出现 manual/demo 不表示现在仍提供该模式。迁移失败不静默重建数据库。

## 调度、告警和导出

每账号并发保护，全局并发上限 3，平台限流、Retry-After 与退避。手动刷新跳过自动间隔，遵守 30 秒防抖与限流；删除/替换连接后丢弃旧 revision 的在途结果。暂停/停用阻止自动采集。

解析校验后保存指标、观测与提醒；失败不更新成功时间。默认提醒线 30/20/10/5%，固定周期用重置锚点，滚动/未知周期用持久化 episode 与恢复条件去重。余额不参与百分比提醒，通知当前仅在应用内。

历史按账号/指标读取 7/30 天曲线、重置分段、默认保留 90 天。后端白名单导出排除凭据、原始响应和认证路径，别名/观测仍需用户自行脱敏。

## 验证边界

优先检查额度方向、缺失、重置、401/403/429、过期、并发/删除竞态、迁移、提醒去重与凭据泄露风险。真实登录/系统凭据操作专项单独启用，不默认查询真实账号。

托盘、安装卸载、拖拽、多屏/DPI、睡眠、开机启动与真实平台对照不能用前端构建替代。待办见 [TASKS](../TASKS.md)。
