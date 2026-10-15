# Codex HTTP 订阅额度

更新：2026-10-16。codex_http 已实现，保持实验性，来源为官方客户端内部端点，可能改变或拒绝第三方调用。

## 使用与认证

在应用添加 Codex · HTTP，授权读取本地登录 → 配置网络 → 测试读取 → 核对并启用保存。普通 OpenAI API Key 不适用。

后台尊重 CODEX_HOME，默认只读 .codex/auth.json 的 access_token 与 account_id；不使用 refresh_token/id_token，不刷新、不改写文件。不支持其他凭据存储后端，不为兼容改写 CLI 配置。默认登录文件不提供独立多账号隔离，切换登录后需重新测试。

固定 GET https://chatgpt.com/backend-api/wham/usage，带 Bearer 与账号头，禁重定向，共享统一代理/超时策略，失败不回退直连。文件和响应限制 64 KiB；应用持有令牌使用 Zeroizing，认证头标 sensitive，底层网络请求头不承诺完整内存擦除。HTTP 与 app-server 不自动切换。

## 字段

| 来源 | 语义 |
|---|---|
| rate_limit.primary_window / secondary_window | 独立窗口，null/缺失不造数 |
| used_percent | 有限 0–100，剩余为 100-used |
| limit_window_seconds | 实际窗口长度，不按 primary/secondary 名字猜周期 |
| reset_at | UTC Unix 秒；缺失不猜 |
| additional_rate_limits | 独立附加池，不合并或平均 |
| allowed、limit_reached、spend_control 等 | 服务端限制不能被剩余百分比覆盖 |
| account_id（若返回） | 核对所选身份；缺失不宣称已核验 |

普通主池触限且窗口明确 used_percent=100 时保留耗尽观测；工作区预算等其他限制继续报错并保留原时间旧值。字段异常、账号不一致、401/403/429、HTML 或无有效窗口返回脱敏状态，不能把空结果当作无限。

base_model_inference 展示为备用额度；浮窗仅在主池任一新鲜窗口剩余为 0、查询成功、备用池也新鲜时替换展示，主池恢复后还原。未知/过期/失败不触发，不改变平台模型或激活状态。

## 验证记录

2026-10-10 经用户授权完成一次真实只读额度查询，获得三个窗口；未将真实响应、身份或令牌写入 fixture/日志/SQLite。此记录仅证明当时该账号和网络可查询，不代表当前服务可用或全部套餐已验证。

合成测试覆盖窗口、附加池、非法/null 字段、账号不一致、过期、文件/响应限制、401/403/429、HTML、重定向与超时。桌面添加/保存/重启/自动刷新和官方页面同时间数值/重置对照仍待验。本次开源整理未查询真实账号。

## 固定来源

- [请求路径](https://github.com/openai/codex/blob/47b80571f0d6587060a580c47f6af0cfaefb0d5c/codex-rs/backend-client/src/client/rate_limit_resets.rs)
- [客户端认证头与映射](https://github.com/openai/codex/blob/47b80571f0d6587060a580c47f6af0cfaefb0d5c/codex-rs/backend-client/src/client.rs)
- [Bearer 与账号头](https://github.com/openai/codex/blob/47b80571f0d6587060a580c47f6af0cfaefb0d5c/codex-rs/model-provider/src/bearer_auth_provider.rs)
- [响应类型](https://github.com/openai/codex/blob/47b80571f0d6587060a580c47f6af0cfaefb0d5c/codex-rs/backend-client/src/types.rs)
- [登录存储](https://github.com/openai/codex/blob/47b80571f0d6587060a580c47f6af0cfaefb0d5c/codex-rs/login/src/auth/storage.rs)

以上为实现时证据，不表示本轮重新验证平台行为。
