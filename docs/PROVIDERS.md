# 平台适配与验证

### Kimi 月额度补充（2026-10-26）

用户实测主窗口和浮窗均只显示 5 小时额度，会员后台另有月额度。[官方会员规则](https://www.kimi.com/help/membership/membership-update-rules)明确：新套餐没有周限额，但新旧套餐均受共享月额度限制，因此不能要求每个账号都显示周窗口。

核对[当前官方额度解析源码](https://github.com/MoonshotAI/kimi-code/blob/main/packages/oauth/src/managed-usage.ts)，新格式使用 `usages.limit_5h`、`limit_7d`、`limit_month_total`、`limit_month_code`，每项读取 0–1 已用比例 `used_ratio` 和 RFC3339 `reset_time`。已补解析会员共享月额度与 Code 月额度，以及新格式的 5 小时/周窗口；有效旧计数窗口优先，避免重复显示和被新格式覆盖。缺失项不生成额度。新老套餐、API Key 与 OAuth 的字段覆盖仍需实测，不能保证仅使用 Code Key 就能查询全部会员窗口。

Context7 本次查得[官方本地 Server API 文档](https://www.kimi.com/code/docs/en/kimi-code-cli/reference/server-api.html)：`GET /api/v1/oauth/usage` 的完整额度需本地 CLI 服务与登录授权；该来源目前未接入，不读取浏览器凭据或复制其他 CLI 登录。当前修复只扩展现有只读接口的返回解析，没有新增认证来源。

2026-10-26 全部 16 个注册入口的源码契约核对、修复与未验证项见 [渠道读取核对记录](providers/channel-read-audit-2026-10-26.md)。这不是新增真实账号验证记录。

更新：2026-10-18。原有平台验证记录如下，新地区入口与证据见后文；阿里百炼及新增地区入口在界面标为实验性。接入状态与真实账号验证范围分别记录，客户端内部接口不保证稳定。

| 产品 / Provider ID | 接入状态 | 来源与指标 | 已记录的验证证据与限制 |
|---|---|---|---|
| Codex HTTP / codex_http | 已接入，用户实测读取成功 | 授权读取 CLI 文件登录，HTTP 订阅额度 | 2026-10-16 用户确认已连接并成功读取；另有 2026-10-10 真实只读查询记录 |
| Codex app-server / codex | 仅保留已有连接 | 官方 CLI stdio 限额 | 新增入口隐藏；历史实测被 PAC-only 阻挡 |
| Claude Code / claude_code | 已接入 | 授权读取 CLI 登录，客户端 usage | 合成测试；真实账号/计划差异待验 |
| DeepSeek API / deepseek_api | 已接入，用户实测读取成功 | 官方余额 API | 2026-10-16 用户确认已连接并成功读取；跨币种及官方页面对照未记录 |
| MiniMax API / minimax_api | 已接入，用户实测读取成功 | 客户端余额接口 | 2026-10-16 用户确认已连接并成功读取；两区域覆盖未记录，无币种不猜测 |
| MiniMax Token Plan / minimax_token_plan | 已接入，用户实测读取成功 | 套餐 remains 接口 | 2026-10-16 用户确认已连接并成功读取；套餐/区域覆盖及重置边界待验 |
| 百炼 API / bailian_api | 已接入 · 实验性 | 官方阿里云账户余额 | 合成测试；签名/权限/真实金额待验，非百炼独立钱包 |
| 百炼个人 Token Plan / bailian_token_plan | 已实测读取成功，登录体验待改善 | 官方 CLI 个人套餐 | 2026-10-16 用户反馈登录 Token 容易过期，影响持续使用；不是查询请求超时；控制台数值/重置对照待验 |

本次用户确认仅覆盖所用账号的连接与读取，不代表所有套餐、地区、重置边界或完整桌面流程已验证。“阿里”的反馈按个人 Token Plan 的官方 CLI 登录来源记录，不扩展为阿里云 AccessKey 账户余额已实测。

详情：[Codex HTTP](providers/codex-http.md)、[Codex app-server](providers/codex.md)、[Claude](providers/claude_code.md)、[DeepSeek](providers/deepseek_api.md)、[MiniMax API](providers/minimax_api.md)、[MiniMax Token Plan](providers/minimax_token_plan.md)、[百炼 API](providers/bailian_api.md)、[百炼个人 Token Plan](providers/bailian_token_plan.md)。

GLM、Kimi 的编程套餐已加入实验性适配，见下方地区说明；Gemini、Antigravity 未接入。

## 接入要求

集中注册适配器，提交产品/地区/套餐、资料来源与固定提交、认证方式、允许域名、合成/脱敏 fixture、字段映射、解析/错误测试与真实账号验证记录。缺账号则记录待验，继续离线工作。

优先公开只读 API，其次用户授权客户端来源；内部端点明确可能失效。禁止浏览器凭据提取、磁盘扫描登录文件、发送模型请求探测额度、manual/mock 回退或本地 token 推算官方余量。

## 数据、凭据与网络

订阅额度、钱包与本地使用量分开；窗口不平均，余额不算百分比、不跨币种汇总、不猜币种。缺失/过期/权限/接口变化分别处理，HTTP 200 不足以证明有有效指标。只在明确已用百分比时转换剩余，重置缺失不推测。

API 凭据通过短期后端会话测试，系统凭据库存储，不进 SQLite、fixture、日志或导出。CLI 授权只读，不改写文件、不抢占 refresh token。缺少服务端身份字段时不宣称身份已验证，切换 CLI 登录需重新核对。

HTTP 使用统一网络服务；百炼 CLI 沿用自身环境，不支持显式直连/自定义覆盖；Codex app-server 支持 HTTP 代理，拒绝 SOCKS/PAC-only。HTTP 支持 HTTP/SOCKS，不支持 PAC/代理认证，失败不自动直连。

## 验证门槛

合成测试只证明离线逻辑；真实查询及同时间页面/CLI 数值、窗口/重置与失败处理通过，才可记录具体范围 validated。未满足保持 experimental，接口变化后降级并保留旧值与原时间。

记录验证日期、范围、客户端版本和未验证项，不保存真实凭据、邮箱、账号 ID 或敏感截图；不能用一个账号宣称全部套餐/地区支持。

## 新增地区入口（2026-10-18）

新增 `minimax_api_intl`、`minimax_token_plan_intl`、`glm_coding_cn`、`glm_coding_intl`、`kimi_coding_cn`、`kimi_coding_intl`。均为实验性，未使用真实账号验证；不得沿用旧 MiniMax 国内账号的成功结论。旧 MiniMax ID 和地址保持兼容，海外入口固定海外地址。

- MiniMax 海外 API：`https://api.minimax.io/account/query_balance`；海外 Token Plan：`https://www.minimax.io/v1/token_plan/remains`。复用对应产品的既有解析与合成 fixture，不猜测币种。来源：[官方套餐页](https://platform.minimax.io/subscribe/token-plan)、[既有 API 来源](providers/minimax_api.md)。
- GLM 国内 `https://open.bigmodel.cn`，海外 `https://api.z.ai`；GET `/api/monitor/usage/quota/limit`，Authorization 为原始套餐 Key，与官方插件一致。`TOKENS_LIMIT`、`TIME_LIMIT` 的 percentage 按已用百分比换算剩余；缺失不生成有效值，未知类型不猜测，重置仅在 nextResetTime 存在时读取。2026-10-26 补充显式 unit/number 多窗口及 CREDIT_LIMIT，按窗口身份判重，不按重置时间排序猜周期；详见上方核对记录。不包含 API 钱包。来源：[官方插件固定提交 0446d0b](https://github.com/zai-org/zai-coding-plugins/blob/0446d0bb0bc537d97d3ab3664c4b8b9c4a0e1254/plugins/glm-plan-usage/skills/usage-query-skill/scripts/query-usage.mjs)。
- Kimi 国内 `https://api.kimi.com`，海外 `https://api.kimi.ai`；GET `/coding/v1/usages`，Bearer 订阅 Key。遵循官方 CLI 的 usage 与 limits/detail 中 limit、used/remaining 计数结构；零分母、缺字段、负数、矛盾计数、错误日期不伪造余量。没有月窗口就不补造；不读取浏览器认证或自动刷新 OAuth。来源：[官方 CLI 固定提交 9ab1286](https://github.com/MoonshotAI/kimi-cli/blob/9ab1286b8fe4e6bcd116949a27ce5e0ac3389c82/src/kimi_cli/ui/shell/usage.py)、[默认国内地址](https://github.com/MoonshotAI/kimi-cli/blob/9ab1286b8fe4e6bcd116949a27ce5e0ac3389c82/src/kimi_cli/auth/platforms.py)、[官方海外地址说明](https://www.kimi.com/code/docs/en/kimi-code/error-reference.html)。地区和套餐返回结构仍需真实账号对照。

合成 fixture：`fixtures/providers/glm-coding.json`、`kimi-coding.json`；仅自动测试使用。统一 HTTP 层保留 401/403/429、重定向拒绝、超时和响应体大小限制；凭据继续使用系统凭据库。

Context7 已查询，无匹配额度文档；以上使用官方源码和官方页面，不发送模型请求探测额度。

### Kimi Code 国内报错核对（2026-10-26）

用户反馈国内 Code 测试读取提示“接口变化”，尚未取得该次响应，未认定根因或记录读取成功。核对当前[官方 CLI 额度解析](https://github.com/MoonshotAI/kimi-cli/blob/main/src/kimi_cli/ui/shell/usage.py)与[平台地址](https://github.com/MoonshotAI/kimi-cli/blob/main/src/kimi_cli/auth/platforms.py)：国内仍为 GET `https://api.kimi.com/coding/v1/usages`、Bearer 认证。官方计数转换接受数字字符串；本地原解析仅接受 JSON 数字，存在兼容缺口。现为 Kimi 的 limit、used、remaining 增加非负整数字符串支持，保留缺字段、零分母、越界、矛盾计数和无效日期校验，不将缺失字段补成零。回归测试使用合成响应，不代表真实账号验证。Context7 本次已检索，未匹配额度契约；修复依据为官方源码，真实读取仍需复测。

### API 余额补充

`kimi_api_cn` 和 `kimi_api_intl` 独立于 Code 套餐，分别查询 `https://api.moonshot.cn/v1/users/me/balance`、`https://api.moonshot.ai/v1/users/me/balance`，Bearer 开放平台 Key。读取 `code=0` 的 available_balance、可选 voucher_balance/cash_balance，保留现金负余额，不产生百分比。海外 USD 来自[官方余额文档](https://platform.kimi.ai/docs/api/balance)的明确单位；国内文档本次不可读取，默认币种未知，仅在响应 currency 有效时显示币种。国内端点与字段仍需真实账号验证，国际契约不可视为国内实测证据。

GLM 国内/海外 API 钱包 blocker：本次未获得可核验的官方余额接口、认证方式与字段契约。公开插件仅证明套餐额度接口，不能当作钱包余额；社区对钱包端点存在不一致信息，因此暂不加入不可查询的余额入口，也不以本地统计替代。后续需要官方接口说明或用户提供的脱敏请求/响应证据（无需提供完整 Key）。
