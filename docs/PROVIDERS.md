# 平台适配与验证

更新：2026-10-16。当前开放新增连接的七个平台均已接入；阿里百炼两个入口在界面标为实验性。接入状态与真实账号验证范围分别记录，客户端内部接口不保证稳定。

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

Kimi、GLM、Gemini、Antigravity 未接入；图标存在不代表支持。

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
