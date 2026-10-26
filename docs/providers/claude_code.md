# Claude Code 订阅额度

更新：2026-10-16。实验性，真实账号和计划差异待验证。

用户在连接页授权后，Rust 只读默认 .claude/.credentials.json 或绝对 CLAUDE_CONFIG_DIR 下同名文件。只使用 claudeAiOauth.accessToken，不使用 refreshToken、不改写、不抢占刷新。过期引导官方 CLI 重新登录，API Key 不适用。

请求 https://api.anthropic.com/api/oauth/usage，Bearer 与 anthropic-beta: oauth-2025-04-20。这是客户端内部 usage 接口，不保证第三方支持或稳定性。统一网络服务支持系统/HTTP/SOCKS/直连及账号覆盖；禁重定向，代理失败不直连回退。

解析 five_hour、seven_day、seven_day_sonnet、seven_day_opus、seven_day_fable、seven_day_oauth_apps，utilization 为已用百分比，仅有效 0–100 时转换剩余，resets_at 按来源解析；缺失不猜。2026-10-26 补充 limits 中 kind=weekly_scoped、group=weekly 的 Fable/Opus/Sonnet 模型周窗口，读取 percent 并覆盖同模型旧窗口；排除 scope.surface 子限额，拒绝重复身份、越界百分比与无效日期。未知模型及其他动态格式不猜测，无可解析窗口不造数。401/403/429 分别提示认证、权限与限流，失败保留旧观测时间。来源身份未核验，切换 CLI 登录须重新测试。新格式证据与合成回归见 [渠道读取核对记录](channel-read-audit-2026-10-26.md)，真实账号仍待验证。

令牌不进 SQLite、常规前端状态、日志、fixture 或导出；文件/响应各限 64 KiB，读取缓冲区和应用令牌使用后清零。

合成 fixture、登录解析/过期/权限、HTTP 错误映射已有测试；真实账号、官方页面对照与桌面流程待验。本次开源整理不读取真实登录。

来源：[官方认证说明](https://code.claude.com/docs/en/authentication)、[CodexBar 固定请求格式证据](https://github.com/steipete/CodexBar/blob/83f0ca80b78ef2d799239b5ee75062eb6d492670/Sources/CodexBarCore/Providers/Claude/ClaudeOAuth/ClaudeOAuthUsageFetcher.swift)。本项目独立实现。
