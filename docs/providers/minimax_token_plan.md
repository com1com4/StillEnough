# MiniMax Token Plan

2026-10-01：已实现，实验性 / 待真实账号验证。

来源：[官方 FAQ](https://platform.minimax.io/docs/token-plan/faq)、[中国订阅页面](https://platform.minimaxi.com/subscribe/coding-plan)、[官方 CLI 固定提交](https://github.com/MiniMax-AI/cli/tree/06e47c70b76f419196678367dae62acca4c94076) 的 `src/types/api.ts` 和 `src/client/endpoints.ts`。套餐用量与普通 API 钱包分开。

添加账号选择 MiniMax Token Plan，选 `https://www.minimaxi.com`（中国）或 `https://www.minimax.io`（国际），在本机界面输入对应套餐 Key → 测试 → 启用 → 保存。Bearer GET `/v1/token_plan/remains`。不读取 CLI 文件、OAuth refresh token 或浏览器 Cookie。

| 来源字段 | 映射 |
|---|---|
| model_remains 中唯一 general 行 | 通用额度池；不聚合 video 等不同额度 |
| current_interval_remaining_percent | 5 小时剩余百分比，方向为剩余 |
| current_weekly_remaining_percent | 周剩余百分比，方向为剩余 |
| current_interval_status / current_weekly_status | 1/2 为可解析窗口；3/未知显示未知，不从 0/0 或 100 推定不限量 |
| end_time / weekly_end_time | Unix 毫秒转 UTC；不使用 remains_time 作为额度计数 |

数值只接受 0–100，不套用未经验证的 boost；计数不推算 tokens。缺失窗口显示未知，两窗口都无有效值则测试失败；重复 general、格式异常返回 schema_changed。到重置点只标待更新，不恢复 100%。base_resp 非零、401/403/429、超时和重定向均不写入假快照。

接口未返回可靠账号身份，按 Key 绑定。凭据复用 Windows 凭据库；fixture 为合成数据。百分比方向、缺失窗口、金额与额度隔离、HTTP 错误/响应上限测试通过。真实套餐、区域、周 boost、两窗口重置与官方页面对照、Windows 主窗口/浮窗交互均待验证。
