# StillEnough CLI 设计

日期：2026-10-18。状态：设计提案，未实现；本文中的新增路径、命令、字段和协议均为拟议内容。此次不改变 TASKS.md 完成状态，不新增运行依赖。

## 1. 目标与边界

提供 `stillenough-cli`，供 AI Agent 和 PowerShell 用户查询已配置账号的订阅额度、独立窗口重置时间、API 钱包余额，触发刷新并诊断授权、登录及接口异常。默认输出表格，`--json` 输出稳定机器契约。CLI 不创建账号、不接收 Key/Token/Cookie、不导入认证文件、不修改其他 CLI 登录，不发送模型请求探测额度，不提供演示数据、自动路由或续费。

“凭据不进 CLI”指本工具的 CLI 进程、参数、环境配置、标准流和输出中均不接收厂商秘密；凭据只在桌面 Rust 后台短期使用。用户在 UI 授权读取 Codex/Claude/bl 来源，不等于授权任意外部 Agent 控制后台。

## 2. 当前源码依据

已阅读 README.md、AGENTS.md、docs/PRODUCT.md、docs/ARCHITECTURE.md、docs/PROVIDERS.md、TASKS.md，以及以下实现。这里记录当前事实，与后文拟议模块分开。

| 当前路径 / 符号 | 对设计的约束 |
|---|---|
| `src-tauri/Cargo.toml`、`src-tauri/src/lib.rs::run` | 已有 `stillenough_lib`（rlib/staticlib/cdylib），但模块私有，库入口启动 Tauri；不是现成 headless API。 |
| `commands/mod.rs::{AppState,Databases,publish,list_accounts,require_main}` | 单个 `Mutex<Connection>`、settings、Runtime、AtomicU64 revision；变更广播 `state-changed` 和兼容事件。账号读取叠加 refreshing 和动态 stale TTL。敏感命令检查 main 窗口。 |
| `lib.rs` | setup 打开数据库、装配唯一 AppState、启动 scheduler、托盘和浮窗；single-instance 插件当前只把第二次启动转为显示主窗口，没有外部查询 RPC。 |
| `services/connections.rs::{Runtime,fetch,refresh,persist_outcome,start_scheduler}` | 会话、账号 gates、provider_cooldown 和并发 semaphore（3）在内存；refresh 依赖 AppHandle、Tauri runtime、publish；手动刷新有 30 秒防抖和限流；持久化核对连接 revision 并写观测/提醒事务。 |
| `providers/mod.rs::{Provider,FetchError}`、`providers/registry.rs` | trait 含 descriptor/validate_url/fetch/fetch_routed/requires_credential/network_kind；FetchError 为 code/retry_after_seconds；注册八个适配器，包括仅维护旧连接的 codex。CLI 复用注册表，不新增 Provider 分支。 |
| `providers/{codex_http,claude,codex,bailian_cli,deepseek,plans,subscription}.rs` | 文件登录、CLI 子进程与 HTTP 有不同授权/网络约束；解析和状态转换已经存在，不能重写一套 Node 实现。 |
| `infra/sqlite.rs::open_database` | 开库创建目录，启用 foreign_keys，自动跑 001–006 迁移；未显式设置 WAL 或 busy_timeout。不能拿它直接实现只读回退。 |
| `infra/credentials.rs` | Windows keyring，服务名 `com.ai-plan-manager.api-keys`，读取返回 Zeroizing；数据库仅保留不透明引用。 |
| `domain/models.rs::{Account,QuotaMetric,AccountInput,AppSettings}` | camelCase；金额是十进制字符串 remaining，币种在 unit，未知币种为 currency_unknown；cliAuthorized 属于连接；当前没有 Agent 访问设置。 |
| `services/{accounts,network,history,alerts,export}.rs` | 统一账号读取、网络选择、历史、提醒去重和白名单导出；export 当前是文件导出，不是 CLI 查询协议。网络配置含内部地址，不直接输出。 |
| `src/lib/{ipc,types,presentation}.ts` | UI invoke Tauri 命令；metricState 将缺指标判 unknown、TTL/重置到点判 stale；refreshState 显示冷却。CLI 不能依赖 WebView 执行这些函数。 |
| `src-tauri/tauri.conf.json` | identifier 为 com.stillenough.desktop，localData override 沿用 `$LOCALDATA/com.ai-plan-manager.app`，数据库名 ai-plan-manager.sqlite；CLI 不按新品牌另建空库。 |

`src/lib` 的语言、Provider 文案、主题、历史分段和浮窗排序是展示层；不把排序后隐藏的窗口当作完整 CLI 数据。返回所有真实指标，包括备用池，不能据此启用备用池。平台真实验证范围沿用 PROVIDERS.md，不因增加 CLI 升级为 validated。

## 3. 核心架构决策

### 3.1 代码复用与 crate 边界

| 方案 | 收益 | 代价 / 判断 |
|---|---|---|
| 在现有包新增 `src-tauri/src/bin/stillenough-cli.rs`，直接依赖 stillenough_lib | 初期改动少，能共享现有 Rust 类型 | 当前库仍绑定 Tauri/build.rs/桌面依赖，CLI 构建可能要求前端产物和 Windows 桌面工具链；不推荐作为长期分发边界。 |
| 独立 headless CLI crate，自行实现查询/存储 | 易独立发布 | 若复制 Provider、错误和 TTL 规则，会与桌面漂移；拒绝复制业务实现。 |
| 提取无 Tauri 的 core crate，独立薄 CLI crate，桌面作为 host | 解析、持久化、错误、投影和刷新策略一处维护；CLI 不带 WebView | 需要逐步解耦 AppHandle。**推荐此方案**，独立 crate 不意味着独立后台。 |

拟新增根 Cargo workspace，保留 `src-tauri` 包与现有 main/lib 入口；新增 `crates/stillenough-core`、`crates/stillenough-cli`。core 以现有 domain/providers/services 的业务部分及 infra/sqlite 为来源搬迁，保持模块职责和测试，不复制目录。系统凭据与网络实现供桌面 host 使用，CLI 入口仅调用安全投影和只读仓储，不调用凭据函数。infra/autostart、窗口、托盘和 main 窗口授权检查留在桌面。

将 `commands::AppState` 的业务状态提取为拟议 `BackendState`，桌面 AppState 包装同一个 Arc；把 connections 中的 AppHandle 状态定位、spawn、事件广播换成显式后台引用和事件 sink，桌面 sink 继续发原事件。Tauri command 和本机 RPC 都调用同一应用服务。无需第一阶段一次搬完所有业务；先抽取契约、账号投影和只读查询，再抽刷新内核。桌面持有唯一 Runtime，CLI 不创建 Runtime/scheduler。

迁移 workspace 时固定已有依赖版本；保留 001–006 顺序。推荐 workspace 根 Cargo.lock 作为唯一锁文件，迁移已有锁定解析并审查差异；同步 README/CI 的 `--locked --manifest-path src-tauri/Cargo.toml` 用法，避免维护两份会漂移的锁。此处仅规划，不移动锁文件。

### 3.2 进程关系

| 模式 | 与现状的关系 | 决策 |
|---|---|---|
| CLI 进程内直接调用 core 刷新 | 代码相同，但内存状态不同；会绕过 gates、并发 3、冷却、revision 广播 | 不作为正常路径。 |
| IPC 到已运行桌面 | 主窗口、浮窗、托盘、Agent 使用同一个 Runtime 和数据库写入路径 | **推荐主路径**。 |
| 两个独立后台共同写 SQLite | WAL 不能共享内存 cooldown、通知去重触发或设置缓存；UI 也不会自动收到 publish | 拒绝。只允许 CLI 只读缓存回退。 |

Windows 使用拟议命名管道 RPC，不监听 TCP、不用浏览器 invoke、不把 CLI 请求转成任意 Tauri command。每当前用户 SID/会话及固定数据目录派生 endpoint；限制为本机当前交互用户，拒绝远程客户端，设置明确 DACL，客户端核对服务端 SID/会话及端点握手，防止管道抢占。禁止任意 endpoint、SQL、认证路径和可执行路径参数。仅暴露下文命令对应的白名单方法。

协议采用有上限的长度前缀 UTF-8 JSON（建议 1 MiB 请求、16 MiB 响应），握手交换 protocol major/minor、appVersion、schemaVersion、instanceId。major 不兼容返回 version_mismatch，不悄悄读库绕过错误。每请求带 requestId/deadline；服务端限制同时连接数和请求数，不记录原始包。revision 只在 instanceId 内单调，重启不能当作旧状态。

读请求在一致性读事务中获取配置、账号和指标，并在读取前后校验内存 revision，必要时有限重试；网络 await 不持数据库锁。返回一个快照及 capturedAt。刷新先登记 operationId，再执行共享服务；CLI 默认等待完成。超时不声称已撤销后台请求，返回 operationId，可查询结果，重复 requestId 在同一实例有限时保留以避免重复刷新。后台重启后操作记录可丢失，返回 operation_lost，以实际账号观测为准。

### 3.3 授权与凭据

保留 `connections.cli_authorized`：它是每连接“允许后台使用本地厂商 CLI 登录来源”的现有 UI 授权。新增独立 `agent_access` 设置，存于同一 SQLite `app_settings` 表独立 key，避免旧版 AppSettings 保存 JSON 时丢字段。拟议值：`version:1, enabled:false, allowRefresh:false, allowOfflineCache:false, updatedAt`。权限只在 main 窗口设置，CLI 无 grant/enable 命令；默认全部 false，记录缺失、损坏或未知版本一律拒绝。设置写事务完成后更新内存、增加 revision 并广播；失败不启用权限。

校验顺序：端点身份/协议 → 读取最新 Agent 权限 → 校验方法及账号 ID → 查询缓存或检查 allowRefresh → 检查账号 enabled → 对本地来源检查 cli_authorized → 调用统一服务读取 keyring 或授权来源并查询。每次请求及实际排队任务执行前重查权限，不能仅握手时检查。撤销阻止新任务，取消尚未发出的任务；已发送的网络请求无法收回，结果不得继续发给被撤销客户端。读缓存也需要 enabled，离线还需要 allowOfflineCache。

CLI 不从 keyring 读取厂商凭据，不读取 Codex/Claude 认证文件、不运行 bl，不接收 `--token`/stdin 密钥或环境变量密钥。后台继续只读 access token/官方 CLI，不刷新或抢占第三方 refresh token。诊断仅返回 available/missing/expired/denied/unchecked 等枚举；不返回秘密、路径、凭据引用、原始响应、CLI stderr 或请求头。UI 密钥输入现有短期 IPC 会话不扩展为 Agent 接口。

离线权限来自只读数据库的同一读事务，不缓存授权到 CLI 配置文件；文件 ACL 保护数据和权限。该设计保护产品边界，不声称可以隔离拥有同用户文件写权限的恶意程序；更强 Agent 隔离需独立 OS 身份，列为开放问题。

### 3.4 单实例、SQLite 与迁移

桌面现有 single-instance 插件保留。新增数据目录范围的 OS 所有者锁（Windows 命名 mutex，含 SID/会话和规范化目录标识），在开库迁移、启动 scheduler/RPC 前获取，整个 host 生命周期持有。管道在迁移完成后才就绪。旧版无此锁时不得声称跨版本并行安全，升级要求退出旧桌面；冲突返回 owner_conflict，不启动第二后台。CLI 不是桌面第二实例，运行时不会弹主窗口。

桌面 writer 在初始化期设置并确认 `journal_mode=WAL`，每连接设置 foreign_keys 和建议 5 秒 busy_timeout；写事务短小，锁超时返回 database_busy，不无限重试。WAL 允许多个读者，不等于多个调度器。只支持本机用户数据目录，拒绝网络共享数据库；不提供任意 `--database`。checkpoint 由 host 管理，不能删除 -wal/-shm 文件；SQLite 文件、WAL、备份均视为敏感数据。

只读回退用新只读开库入口，禁止调用当前 open_database，禁止 create_dir、建库、迁移和 `immutable=1`。先读 schema_migrations；仅支持明确兼容的 schema 范围，较新返回 schema_incompatible，较旧返回 migration_required，让用户启动匹配桌面升级。只读 WAL 环境必须验证 -shm 可访问；失败报 cache_unavailable，不能复制主库漏掉 WAL。没有数据库返回 not_initialized，不能凭空创建账号。

迁移只有持所有者锁的桌面 host 执行；先拒绝未来版本，再执行现有顺序迁移，备份需使用一致性备份策略，失败保留原库并停止 host，不静默重建。CLI 不提供 migrate 命令。下一次 schema 变更新增版本，不重编号；WAL 配置不伪称当前已有。

## 4. 命令与输出契约

### 4.1 命令表

所有业务命令支持 `--json`、`--timeout <seconds>`（默认 30，范围 1–300）；表格为默认，`--no-color` 禁止 ANSI，重定向时自动禁用颜色。JSON 模式 stdout 仅一个完整对象加换行，进度只在 stderr，stderr 同样脱敏；能捕获的错误也输出 JSON envelope。`--help`/`--version` 是纯文本元命令，机器版本使用 `version --json`。无交互输入、分页器和确认提示。

| 命令 | 参数与用途 | data.kind / 结果 | 访问与失败 |
|---|---|---|---|
| `version` | 无；查询二进制与协议版本 | version，metadata | 无需桌面或授权 |
| `providers` | 可选 `--provider <id>`；注册表支持范围 | providers，providers[] | 本地静态元数据；不查询账号，不声称已实测 |
| `accounts list` | 可选 `--provider <id>`，`--offline` | accounts，accounts[]（metrics 为空） | 在线需 enabled；offline 需离线授权 |
| `quota` | 可选 `--account <id>` 或 `--provider <id>`，`--offline`，`--require-fresh` | snapshot，accounts[] 含所有额度/余额/重置 | 默认读缓存，不隐式网络；严格新鲜度失败仍输出旧值 |
| `refresh` | 必选 `--account <id>` 或 `--all`，可选 `--provider <id>` 仅配 --all | refresh，operationId、accounts[] | 需在线和 allowRefresh；默认等待持久化完成；不接受 --offline/--force |
| `operations get <operationId>` | 查询刷新是否完成，不重新请求厂商 | operation，operationId、operationState、accounts[] | 在线授权；未知或实例重启返回 operation_lost |
| `doctor` | 可选 `--account <id>`；`--live` 仅与账号配合；`--offline` 不可与 live 同用 | diagnostics，checks[] | 默认不网络/不打开凭据，只检查协议、schema、授权、上次错误；live 走共享刷新服务并持久化结果，需 allowRefresh |

账号 ID 使用 accounts list 输出的不透明 ID，不以别名定位；未知 ID 返回 not_found，非法 provider 返回 invalid_arguments；有效 provider 没有配置账号返回空数组。refresh --all 仅选择已启用账号，禁用账号不请求；显式请求禁用账号返回 account_disabled。空选择不网络，结果成功并附 no_accounts 警告。--provider 和 --account 在查询中互斥，避免静默忽略筛选。

在线 refresh 保留现有手动语义：全局 collectionPaused 只暂停 scheduler，显式手动刷新仍可执行并在 warnings 标明；单账号 enabled=false 则拒绝。30 秒防抖不允许绕过；同账号已有任务返回 busy/operationId（若可关联），不启动第二次；平台限流返回 nextRetryAt。refresh --all 使用同一并发 3 和 cooldown，失败账号不阻断其他账号。

### 4.2 状态语义

把“最近采集状态”与“指标新鲜度”分开。账号 `status` 为归一错误状态，metric `freshness` 为 current/unknown/expired；`expired` 在指标层表示 TTL 或已到重置边界，在账号层表示已确认登录过期。`errorCode` 保留白名单内部码，不能用本地化 message 判断逻辑。

| status | 来源 / 行为 |
|---|---|
| ok | 最近成功且有有效指标；具体指标仍独立检查 freshness |
| unknown | idle、无观测或未知业务状态；无指标输出空数组，无百分比不合成 0 |
| expired | local_login_expired 等明确登录到期证据；旧值原样保留并标 freshness |
| auth-required | auth_required、cli_auth_required、local_credentials_unavailable；不能将所有 401 宣称为过期 |
| forbidden | permission_denied、cli_authorization_required、Agent 权限未启用；scope 区分 provider/local-policy |
| api-changed | schema_changed；结构变化无有效解析，不返回空白成功 |
| rate-limited | rate_limited，返回 nextRetryAt，遵守 Retry-After |
| network-error | network_error、代理/TLS/连接错误；不自动换网络 |
| unsupported | unsupported、套餐不支持、CLI 版本不支持等；仅保留有证据的 errorCode |
| busy / paused / error | 刷新中、账号停用、其余白名单故障；未映射代码用 error，不吞成 ok |

`cli_quota_unavailable` 归 unknown；identity mismatch 归 error 并提示在 UI 核对身份；codex_usage_blocked 归 forbidden。实现前逐项列举各适配器现有码并测试，不按字符串包含 “expired” 猜语义。

指标保留 remaining/limit 十进制文本和 remainingPercent 数值或 null；钱包百分比恒 null，unit 可 currency_unknown，不换币种、不合计；不把 API 钱包当订阅额度。未知值表格显示“未知”，过期数值显示“旧观测”及原 observedAt；重置未知为 null/“未知”，到点只标过期，不补 100%。TTL 使用当前 list_accounts 的 max(refreshMinutes*120,900) 秒，提取共享 Rust freshness 规则，与 presentation.ts 的缺值、无效时间、TTL、reset 边界做一致性用例。前端后续采用后端状态，避免双份业务判断。

### 4.3 JSON Schema（v1）

以下为完整 envelope schema；按命令使用 data.kind 和对应数组。数组统一存在，不适用时为空；metadata 为版本信息，非版本命令为空。服务端和 CLI 共享 DTO，使用端验证 schema；运行时请求拒绝未知字段。响应后续可新增字段，消费者忽略不认识的字段，破坏性变更升 schemaVersion/protocol major。

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "StillEnoughCliResponseV1",
  "type": "object",
  "required": ["schemaVersion", "requestId", "command", "capturedAt", "transport", "instanceId", "revision", "outcome", "data", "errors", "warnings"],
  "properties": {
    "schemaVersion": {"const": 1},
    "requestId": {"type": "string"},
    "command": {"type": "string"},
    "capturedAt": {"type": "string", "format": "date-time"},
    "transport": {"enum": ["ipc", "offline-cache", "local"]},
    "instanceId": {"type": ["string", "null"]},
    "revision": {"type": ["integer", "null"], "minimum": 0},
    "outcome": {"enum": ["ok", "partial", "error"]},
    "data": {
      "type": "object",
      "required": ["kind", "accounts", "providers", "checks", "operationId", "operationState", "metadata"],
      "properties": {
        "kind": {"enum": ["version", "providers", "accounts", "snapshot", "refresh", "operation", "diagnostics"]},
        "accounts": {"type": "array", "items": {"$ref": "#/$defs/account"}},
        "providers": {"type": "array", "items": {"$ref": "#/$defs/provider"}},
        "checks": {"type": "array", "items": {"$ref": "#/$defs/check"}},
        "operationId": {"type": ["string", "null"]},
        "operationState": {"enum": [null, "running", "completed", "failed"]},
        "metadata": {"type": "object", "additionalProperties": {"type": "string"}}
      }
    },
    "errors": {"type": "array", "items": {"$ref": "#/$defs/error"}},
    "warnings": {"type": "array", "items": {"type": "string"}}
  },
  "$defs": {
    "status": {"enum": ["ok", "unknown", "expired", "auth-required", "forbidden", "api-changed", "rate-limited", "network-error", "unsupported", "busy", "paused", "error"]},
    "nullableTime": {"type": ["string", "null"], "format": "date-time"},
    "decimal": {"type": ["string", "null"], "pattern": "^-?[0-9]+(\\.[0-9]+)?$"},
    "metric": {
      "type": "object",
      "required": ["metricId", "label", "kind", "unit", "remaining", "limit", "remainingPercent", "resetsAt", "source", "observedAt", "staleAfterSeconds", "freshness"],
      "properties": {
        "metricId": {"type": "string"}, "label": {"type": "string"},
        "kind": {"enum": ["quota", "balance"]}, "unit": {"type": "string"},
        "remaining": {"$ref": "#/$defs/decimal"}, "limit": {"$ref": "#/$defs/decimal"},
        "remainingPercent": {"type": ["number", "null"], "minimum": 0, "maximum": 100},
        "resetsAt": {"$ref": "#/$defs/nullableTime"},
        "source": {"enum": ["official_api", "client_endpoint"]},
        "observedAt": {"type": "string", "format": "date-time"},
        "staleAfterSeconds": {"type": "integer", "minimum": 0},
        "freshness": {"enum": ["current", "unknown", "expired"]}
      },
      "allOf": [{"if": {"properties": {"kind": {"const": "balance"}}}, "then": {"properties": {"remainingPercent": {"type": "null"}}}}]
    },
    "account": {
      "type": "object",
      "required": ["id", "providerId", "alias", "enabled", "status", "errorCode", "lastAttemptAt", "lastSuccessAt", "nextRetryAt", "refreshing", "metrics"],
      "properties": {
        "id": {"type": "string"}, "providerId": {"type": "string"}, "alias": {"type": "string"},
        "enabled": {"type": "boolean"}, "status": {"$ref": "#/$defs/status"},
        "errorCode": {"type": ["string", "null"]},
        "lastAttemptAt": {"$ref": "#/$defs/nullableTime"}, "lastSuccessAt": {"$ref": "#/$defs/nullableTime"},
        "nextRetryAt": {"$ref": "#/$defs/nullableTime"}, "refreshing": {"type": "boolean"},
        "metrics": {"type": "array", "items": {"$ref": "#/$defs/metric"}}
      }
    },
    "provider": {
      "type": "object", "required": ["id", "name", "supported", "capability", "status", "limitation"],
      "properties": {
        "id": {"type": "string"}, "name": {"type": "string"}, "supported": {"type": "boolean"},
        "capability": {"type": "string"}, "status": {"type": "string"}, "limitation": {"type": "string"}
      }
    },
    "check": {
      "type": "object", "required": ["name", "state", "code"],
      "properties": {"name": {"type": "string"}, "state": {"enum": ["ok", "missing", "expired", "denied", "unchecked", "error"]}, "code": {"type": ["string", "null"]}}
    },
    "error": {
      "type": "object", "required": ["code", "status", "scope", "accountId", "retryable", "nextRetryAt", "message"],
      "properties": {
        "code": {"type": "string"}, "status": {"$ref": "#/$defs/status"},
        "scope": {"enum": ["request", "local-policy", "provider", "storage", "transport"]},
        "accountId": {"type": ["string", "null"]}, "retryable": {"type": "boolean"},
        "nextRetryAt": {"$ref": "#/$defs/nullableTime"}, "message": {"type": "string"}
      }
    }
  }
}
```

额外契约：时间统一 RFC3339 UTC；offline 的 instanceId/revision 为 null、refreshing=false（表示无法观测在途，warnings 包含 runtime_unavailable），不伪造实例 revision。metadata 版本命令至少含 cliVersion/protocolVersion/coreVersion。message 来自固定文案，禁止拼接底层异常。accounts list 中空 metrics 表示未请求指标，不能据此推断没额度。额度已耗尽的真实 0 与 null 必须区别。

### 4.4 退出码与错误处理

| 退出码 | 语义与典型 code |
|---|---|
| 0 | 命令按契约成功；普通缓存查询可返回 unknown/旧值，仍须检查指标状态；operation running 也可成功读取 |
| 2 | 参数错误 invalid_arguments |
| 3 | 无可用后台/数据 desktop_unavailable、not_initialized、cache_unavailable |
| 4 | 本地授权或认证不满足 agent_access_denied、auth_required、local_login_expired |
| 5 | 厂商拒绝或接口/套餐不兼容 permission_denied、schema_changed、unsupported |
| 6 | 暂时不可执行 rate_limited、refresh_cooldown、refresh_in_progress、account_disabled、database_busy |
| 7 | 超时/网络错误 deadline_exceeded、network_error |
| 8 | 多账号刷新/检查部分成功 partial_failure |
| 9 | 协议/schema/所有者不兼容 version_mismatch、schema_incompatible、migration_required、owner_conflict |
| 10 | 内部/存储不可恢复错误 internal_error、database_error |
| 11 | quota --require-fresh 未满足 fresh_required（任一选中指标非 current 或账号无指标） |
| 12 | 指定对象不存在 not_found、operation_lost |

命令请求层失败 outcome=error；批量刷新至少一个成功且至少一个失败 outcome=partial/退出 8；全部失败 outcome=error，采用固定优先级 10 > 9 > 4 > 5 > 7 > 6 > 12 > 3 > 11，明细保留每账号 code。单账号刷新失败不因旧观测存在而返回成功。普通 quota 查询的 outcome 表示读取是否成功，账号故障由 status 表示；--require-fresh 仅严格检查数值新鲜度，不能代替 refresh 的执行成功判断。doctor 存在失败检查时按相同规则非零，unchecked 只警告、不当作认证成功。进程被强制终止/管道被关闭不保证完整 JSON，Agent 应视解析失败为传输失败，不能当作无额度。

## 5. 安装、分发与降级

下列均为计划命令，目前 package.json 没有 CLI scripts，也没有 CLI crate；不应现在照此验收成功。

| 方式 | 计划接口 | 约束 |
|---|---|---|
| 源码开发 | 新增 `pnpm cli -- quota --json`、`pnpm cli:build` | script 分别委托 cargo run/build --manifest-path crates/stillenough-cli/Cargo.toml --bin stillenough-cli（run 使用 -- 转发）；不经 Vite、不自动启动桌面。 |
| Cargo 本机安装 | `cargo install --path .\crates\stillenough-cli --locked` | workspace 完成后验证；Rust MSVC 工具链，CLI 不要求 WebView2；不能按当前 src-tauri 路径安装后假定已有 CLI。 |
| Windows release | 桌面 ZIP/安装包附带 stillenough-cli.exe；另发独立 CLI ZIP | 明确 x64 先行，记录哈希/版本；安装目录默认不强改全局 PATH，提供完整路径调用。 |
| npm 全局包 | 拟发布独立非 private CLI wrapper，`npm install -g @stillenough/cli`（包名可用性待确认） | 当前根包 private=true，不直接发布；wrapper 只启动匹配版本原生二进制，不存凭据、不实现 Provider；Windows 平台包带二进制，避免 postinstall 下载执行。Node 为此渠道依赖，纯 ZIP 不需要。 |
| PowerShell / .cmd | `& 'C:\Program Files\StillEnough\stillenough-cli.exe' quota --json`；npm bin 生成 stillenough-cli.cmd | wrapper 转发 argv/stdout/stderr 和退出码，不做 shell 拼接；用 .cmd 可避免 npm .ps1 执行策略阻塞。独立 ZIP 可提供同名 .cmd 调用同目录 exe，准确引用含空格路径，禁止 eval。 |

PowerShell Agent 示例：`$result = & stillenough-cli.cmd quota --json; $exit = $LASTEXITCODE; $data = $result | ConvertFrom-Json`。先保存退出码，再解析；JSON 是 UTF-8 无 BOM。老版 PowerShell 对重定向编码的影响纳入实测，文档不建议未经验证的重编码链。npm wrapper 必须保留 Ctrl+C 和原生退出码。桌面 stillenough.exe 与 CLI 名称不同，避免 Windows GUI subsystem 吞掉控制台输出。

降级决策：

1. 桌面运行且权限满足：IPC 读取/刷新；主窗口隐藏、浮窗关闭、托盘常驻均不影响。
2. IPC 明确 endpoint 不存在：查询类命令可自动回退已授权只读缓存，warnings 标记 desktop_unavailable；`--offline` 则直接只读，绝不网络。协议不兼容、拒绝授权或端点身份异常不得自动回退。
3. 桌面未运行：refresh、doctor --live、operations 返回 desktop_unavailable，不启动隐藏 daemon、不打开 UI、不直接联网。用户自行启动桌面即可恢复；doctor 可给出固定操作提示。
4. 桌面未安装但保留兼容数据库及离线授权：允许只读；无库返回 not_initialized；providers/version 仍可用。
5. 未安装/未配置且需要真实查询：引导安装桌面并在 UI 配置账号/授权。独立 CLI 第一版不承诺无桌面首次配置或无人值守 headless 刷新。

## 6. 分阶段实施计划（均未开始）

每阶段都保持桌面可运行，增加一个可单独验收的结果。不得把以下计划写入已完成。

| 阶段 | 产出与可运行验收 | 验证 |
|---|---|---|
| P1：契约与本地命令 | 建 workspace/core/CLI 最小边界，只抽 Provider 描述投影、DTO/状态规范；version/providers 有表格和 --json，未知参数退出 2；不读数据库、不接凭据；桌面仍能启动。 | JSON schema/退出码/秘密哨兵用例；pnpm test/lint/typecheck，cargo fmt/check/clippy/test；Windows 控制台 Unicode/重定向 smoke。 |
| P2：只读离线查询与 UI 授权 | 新增 main-only agent_access 设置，core 只读仓储和 freshness，accounts/quota/doctor --offline 可用；未授权、无库、旧/新 schema 均明确失败；旧值不补额。 | 临时数据库迁移快照、缺/损坏授权、金额精度、0/null、reset 边界、Unicode；pnpm test/lint/typecheck，cargo fmt/clippy/test；重启授权恢复实测。 |
| P3：在线快照 IPC | 命名管道、身份/协议检查、限长、超时、默认查询 IPC；主/浮窗与 CLI 同一 revision；启动所有者锁、WAL/只读并发验证。 | 两 CLI 并发读取、冒充端点、远程拒绝、断管道、迁移失败、锁竞争、撤销不回退；pnpm test/lint，cargo fmt/clippy/test，真实 Windows 主/浮窗 smoke。 |
| P4：刷新与诊断 | 解耦 connections 刷新内核/事件 sink，refresh/operations/doctor --live；共用 Runtime、gates、网络、持久化和提醒；不允许第二 writer。 | 合成 HTTP/CLI 传输下的 401/403/429/schema/超时、并发 3、防抖、删除/替换 revision、撤销排队任务、请求去重；pnpm test/lint/typecheck，cargo fmt/clippy/test；授权真实账号只读核对另记。 |
| P5：分发与稳定性 | 源码 scripts、cargo install、CLI ZIP、桌面附带二进制、npm 平台 wrapper；版本/兼容矩阵与 Agent 使用说明；全流程不用 bash。 | 干净 Windows x64 普通用户下，空格/中文路径、PowerShell 5.1/7、cmd、无 Node/无 WebView2 的独立 ZIP、本地 npm 包安装、卸载保留数据；pnpm test/lint/build，cargo fmt/clippy/test 和 release 安装 smoke。 |

常规 Rust 命令沿用 README：`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --manifest-path src-tauri/Cargo.toml --locked`、`cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings`、`cargo test --manifest-path src-tauri/Cargo.toml --locked`；workspace 建立后新增根 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`，覆盖新增 crates。前端命令继续从根 PowerShell 执行。测试替身仅在自动测试中使用，不能进入可运行产品数据。

每阶段交付记录命令、结果、未验证项和下一步；现 TASKS.md 已记录 pnpm 权限限制及 Rust 百炼子进程用例历史失败，应先复核基线、如实列出失败，不能用新增测试通过声称全仓库通过。真实平台验证与 Windows 桌面测试单列，合成测试不替代。

## 7. 风险、测试策略与开放问题

- **安全边界**：管道同用户权限并不识别“可信 Agent”；用户启用即允许同用户程序调用白名单 API。别名和指标仍有隐私，终端日志与 Agent 对话可能留存；表格转义控制字符/ANSI，JSON 按规范编码。输出只用白名单 DTO，不直接 serialize Account、AppSettings 或底层错误。测试用秘密哨兵注入 header/body/CLI stderr/路径/凭据引用，断言两条标准流、日志、schema、导出和数据库均无秘密；厂商原始错误禁止直接透传。
- **维护漂移**：一个注册表、一个 refresh 内核、一个 freshness/错误映射、一个迁移实现；表格只是同一 DTO 的 formatter。契约快照测试加 UI/CLI 共同输入的边界用例，schema 版本独立于产品版本；禁止复制 TS presentation 业务逻辑到 CLI。
- **Windows 坑**：用户 SID/会话、提权与非提权、服务账号、终端编码、.cmd 引号、PATH 冲突、杀毒锁文件、MSVC 运行依赖、mutex abandoned 状态、睡眠恢复、只读 WAL/shm 权限。CLI 首版限定当前交互用户，提权/跨会话不自动探测其他账号。锁持有者死亡后先检查数据库完整状态再恢复，不凭 PID 文件判断存活。
- **故障测试**：临时文件库测试 WAL 并发、事务快照、busy_timeout、未来 schema 拒绝、迁移中断和磁盘满；可控时钟测试 TTL、reset、Retry-After；受控传输测试断管道/服务重启/超时后查询 operation。刷新错误保留旧成功时间和指标；提醒去重与历史事务仍走既有测试。
- **开放决策**：管道具体 Rust 依赖及 SID 校验方案在 P3 前以当前官方文档验证；此提案不假定某个库已支持安全 DACL。npm 包名、签名证书、平台包发布权限待确认；不在本任务发布。需要无桌面常驻 headless host 时另开设计，必须复用同一所有者锁和 Runtime，并重新设计 UI 授权供给，不能临时允许 CLI 自己读凭据。
- **兼容范围**：先支持与桌面同主版本协议和当前 schema；更宽的跨版本范围必须有双版本测试证据。macOS/Linux 不承诺支持。P2 离线权限通过同用户数据库保护，不抵御该用户直接修改数据库；需要更严格的撤销/防篡改模型时，应取消离线读取并强制 host 在线。

## 8. 本次设计检查记录

本次只阅读当前源码和仓库文档、编写本文件并检查 Markdown/内嵌 JSON Schema 语法及工作区差异；没有实现 CLI、执行真实账号查询或运行测试套件。crate、WAL、锁、IPC、权限开关和分发命令均为待实施设计，不能作为现有能力宣传。
