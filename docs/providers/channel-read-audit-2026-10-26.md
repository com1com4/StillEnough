# 渠道读取契约核对（2026-10-26）

覆盖当前注册的 16 个 Provider ID，核对请求地址、认证、指标方向、窗口身份、重置字段与失败行为。本次是源码和合成响应核对，没有使用真实账号查询，不扩大任何渠道的实测范围。官方资料可确认的部分与仍待真实账号验证的兼容处理分别记录。

## 逐渠道结果

| Provider ID | 请求和认证 | 核对结果 / 处理 |
|---|---|---|
| `kimi_coding_cn`、`kimi_coding_intl` | 对应 `api.kimi.com` / `api.kimi.ai`，GET `/coding/v1/usages`，Bearer 套餐 Key | 支持数字与数字字符串计数。usage 为总周窗口，limits/detail 为独立窗口，按 remaining/limit 或 (limit-used)/limit 转剩余百分比。保留缺失、零分母、矛盾计数和日期检查。 |
| `glm_coding_cn`、`glm_coding_intl` | 对应 `open.bigmodel.cn` / `api.z.ai`，GET `/api/monitor/usage/quota/limit`，原始 Authorization Key | 路径、认证、percentage 已用方向一致。修复同类型多个窗口被拒绝的问题：显式 unit=3 + number 标识小时窗口，unit=6 标识周窗口；CREDIT_LIMIT 单独标为 Credit 额度。按身份区分，不按重置先后猜周期；不明确的重复行仍拒绝。保留 TIME_LIMIT 工具调用额度。 |
| `minimax_token_plan`、`minimax_token_plan_intl` | 已配置的对应区域 www 域名，GET `/v1/token_plan/remains`，Bearer 套餐 Key | 官方 CLI 仍采用现有路径，保留已实测通路；取 general 与 remaining_percent。官方明确 status=2 为耗尽，允许 1/2 并验证实际百分比。status=3/缺失不伪装 100%。 |
| `minimax_api`、`minimax_api_intl` | 对应允许的 API 域名，GET `/account/query_balance`，Bearer 普通 Key | 按官方 CLI 的 available_amount/cash_balance 十进制字符串读取。来源无币种则未知，负余额不截零。 |
| `deepseek_api` | GET `https://api.deepseek.com/user/balance`，Bearer Key | 路径、balance_infos、total_balance 与方向一致。保留多币种与十进制文本，缺失币种不默认 CNY，不隐藏零余额钱包。is_available=false 不代表认证失效。 |
| `kimi_api_cn`、`kimi_api_intl` | 对应 `api.moonshot.cn` / `api.moonshot.ai`，GET `/v1/users/me/balance`，Bearer 钱包 Key | 与官方海外余额文档一致，code=0、available_balance 与可选 voucher_balance/cash_balance；国内币种仍按响应确认，不能拿海外 USD 当国内证据。 |
| `claude_code` | GET `https://api.anthropic.com/api/oauth/usage`，本地授权 access token + `anthropic-beta: oauth-2025-04-20` | 地址、认证和旧窗口 utilization 一致。新增 seven_day_fable 及 limits[] 中 weekly_scoped / weekly 的模型 percent 解析；新格式覆盖同模型旧窗口，沿用原 metric ID；排除 scope.surface 子限额。已知模型为 Fable/Opus/Sonnet，未知模型不猜。 |
| `codex_http` | GET `https://chatgpt.com/backend-api/wham/usage`，本地授权 Bearer token + ChatGPT-Account-Id | 主窗口 used_percent、limit_window_seconds、秒级 reset_at 一致。我们继续保留 additional_rate_limits、账号匹配、服务端限制及备用池处理。重置券和 credits 属于尚未接入的新指标，不把 credits 当官方美元钱包。 |
| `codex` | 官方 app-server 的 account/read、account/rateLimits/read | 保留现有兼容入口和 RPC 测试，不切换认证来源。 |
| `bailian_api` | 阿里云 QueryAccountBalance，AccessKey 签名 | 沿用既有官方账户余额契约和签名测试，AvailableCashAmount 与 AvailableAmount（含信用）分开。不是百炼专属余额。 |
| `bailian_token_plan` | 授权调用官方 bl CLI usage token-plan | 保留 CLI 身份/能力探测、月及其他明确窗口、0–1 已用比例换算。登录过期不刷新其他工具的令牌。 |

## 官方资料复核

- [Kimi CLI usage.py](https://github.com/MoonshotAI/kimi-cli/blob/main/src/kimi_cli/ui/shell/usage.py)、[平台地址](https://github.com/MoonshotAI/kimi-cli/blob/main/src/kimi_cli/auth/platforms.py)：前一轮已核对同一地址与计数字符串转换。
- [MiniMax CLI endpoints.ts](https://github.com/MiniMax-AI/cli/blob/main/src/client/endpoints.ts)、[api.ts](https://github.com/MiniMax-AI/cli/blob/main/src/types/api.ts)：本轮直接读取，确认 token_plan 路径、余额路径、百分比字段、状态 1/2/3。Context7 先 resolve MiniMax，再查询官方 CLI 文档，得到相同路径和状态说明。weekly_boost_permille 为另一个显示乘数；本轮保留基础百分比，不扩展 0–100 模型。
- [GLM 官方查询插件](https://github.com/zai-org/zai-coding-plugins/blob/main/plugins/glm-plan-usage/skills/usage-query-skill/scripts/query-usage.mjs)：本轮直接读取，确认端点、原始 Authorization、percentage 语义。unit 窗口映射与 CREDIT_LIMIT 属于兼容处理，该官方插件未明确其契约，尚无本项目真实账号验证。
- [Moonshot 海外余额文档](https://platform.kimi.ai/docs/api/balance)：前一轮核对字段、code=0、Bearer 和海外 USD。该文档不能证明国内真实响应。

## 兼容边界与待实测

缺失计数不回填 0/1，无效 percentage 不回填 0；不依据 reset 排序猜 GLM 周期，不吞掉重复窗口、不默认币种或复制原始错误响应。当前后台的错误码、响应大小限制、重定向拒绝、旧观测保留和凭据保护继续生效。

新增合成 fixture：`glm-coding-windows.json`、`claude-scoped-usage.json`。回归覆盖 GLM 周窗口先重置及行重排、重复/模糊窗口、缺失百分比、Credit 额度；Claude 新旧覆盖、独立新格式、0% 已用、无重置时间、surface 排除、重复/越界/无效日期；保留前一轮 Kimi 字符串测试。

Windows 真实账号待验：Kimi 国内重新测试；GLM 老/新套餐的显式窗口与官方页面对照；Claude 旧/新响应及模型窗口；MiniMax 已有账号持续读取、状态=2 耗尽与周加成显示。源码核对不代表这几项已通过。运行修复版：`pnpm tauri dev`，在原账号点击测试读取或刷新。本轮不构建发布包。

## 实际检查

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`git diff --check` 通过。
- `cargo test --workspace --locked -- --test-threads=1`：普通 Windows 环境 130 项通过，7 项真实环境专项忽略。沙箱内初跑 Provider 测试时百炼子进程测试失败，普通环境复核该用例及完整工作区均通过。
- 未修改前端，未运行前端构建或桌面交互；没有使用真实 Key、CLI 登录或浏览器凭据进行查询。以上成功不代表真实渠道读取已验证。
