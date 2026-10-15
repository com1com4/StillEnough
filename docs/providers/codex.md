# Codex app-server

更新：2026-10-16。实验性；仅保留已有账号管理/查询，新增入口隐藏。新连接使用 [Codex HTTP](codex-http.md)。

## 当前实现

授权本地 CLI 来源后，后台通过原生 Codex CLI app-server stdio 依次 initialize、account/read（refreshToken=false）、account/rateLimits/read。仅支持 ChatGPT 登录；不读取 auth.json、不复制 Token、不执行登录/主动刷新，不发送模型请求。

优先解析 rateLimitsByLimitId，避免与旧 rateLimits 重复。usedPercent 是已用百分比，有限 0–100 才转换剩余；窗口分别显示，resetsAt 缺失不猜。无数据、协议变化或未登录返回错误，不造数。切换 CLI 身份后需重新测试。

支持继承环境、Windows 静态代理、自定义 HTTP 与显式直连；子进程自定义 SOCKS 和 PAC-only 配置拒绝，不绕过。

## 验证

历史本机 CLI 0.159.3 实测被 PAC-only 网络校验阻挡，未取得真实额度；合成 stdio 测试覆盖握手、账号类型、错误、超时、输出上限、映射与网络策略。真实账号、完整桌面流程仍待验证。

## 来源

- [官方协议定义固定提交](https://github.com/openai/codex/blob/41d3dc56a0e1de47e30a9585c1b49253c082f8f7/codex-rs/app-server-protocol/src/protocol/common.rs)
- [官方后端限额客户端固定提交](https://github.com/openai/codex/blob/dc21b46aea67947c3ab1b574cabcd79ff7084c73/codex-rs/backend-client/src/client/rate_limit_resets.rs)

公开源码不等于面向第三方的稳定接口承诺。fixture 仅为合成解析证据。
