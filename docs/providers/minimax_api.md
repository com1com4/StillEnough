# MiniMax API 钱包

2026-10-01：已实现，实验性 / 待真实账号验证。

来源为 [MiniMax 官方 CLI 固定提交](https://github.com/MiniMax-AI/cli/tree/06e47c70b76f419196678367dae62acca4c94076)：`src/client/endpoints.ts`、`src/config/schema.ts`、`src/types/api.ts`、`src/output/balance.ts`。这是客户端账户接口，不宣称为稳定公开 API。

在“添加账号”选择 MiniMax API，选中国 `https://api.minimax.cn` 或国际 `https://api.minimax.io`，在界面输入对应区域的 `sk-api-` Key，测试成功后启用并保存。只执行 Bearer GET `/account/query_balance`，不调用模型。旧格式 Key 尚未验证，不做自动回退。

| 来源字段 | 显示 | 语义 |
|---|---|---|
| available_amount | 可用金额 | 十进制原值，不转成百分比 |
| cash_balance | 现金余额 | 十进制原值，0 是有效余额 |
| base_resp.status_code | 错误状态 | 非零不保存快照 |

当前映射与合成 fixture 未提供币种：内部单位 `currency_unknown`，主窗口及连接测试显示金额和“未知币种”，浮窗只显示数值；不依据区域猜测币种，不展示代金券/信用为现金。历史沿用原有行为。来源为 `client_endpoint`。接口无账号身份字段，按配置 Key 绑定且需用户核对；更换区域或 Key 必须重新测试。

凭据仅进入短期后端会话和 Windows 凭据库，不进入 SQLite、日志或 fixture。固定域名、拒绝重定向、15 秒超时、64 KiB 响应上限；401/403/429 和错误响应只返回脱敏状态，失败保留上次快照和时间。

`fixtures/providers/minimax-balance.json` 为合成数据，非真实响应。离线解析和 HTTP 失败测试通过；真实账号、两区域金额/币种对照和 Windows 表单交互未验证。下一步在本机界面录入 Key，与官方账户页同时对照；不要发送 Key 到聊天。
