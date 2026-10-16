# DeepSeek API 余额适配器

更新：2026-10-16。状态：**已接入，用户实测读取成功**。用户确认已连接真实账号并读取余额；具体币种覆盖、官方页面对照及完整桌面流程的验证范围仍需补充。

## 产品、来源与范围

产品为 DeepSeek API 账号钱包，不是 Coding Plan 或订阅额度。官方来源：[查询余额](https://api-docs.deepseek.com/zh-cn/api/get-user-balance/)。仅执行 `GET https://api.deepseek.com/user/balance`，认证为 Bearer API Key；不调用聊天或模型接口。

地址只接受 `https://api.deepseek.com`，不接受任意兼容网关、路径、query、userinfo 或 fragment；禁止重定向，保留 TLS 校验。客户端总超时 15 秒、连接超时 8 秒，响应上限 64 KiB。Key 输入通过独立 IPC 进入 10 分钟后端会话，输入提交后清空；保存才写 Windows Credential Manager，不存 SQLite，不回显、不打印原始错误响应。

## 字段映射

| 原字段 | 标准展示 | 规则 |
|---|---|---|
| balance_infos[].currency | unit 与余额标签 | 按共享 ISO 4217 代码表校验并转大写；每币种独立，不汇总换汇 |
| balance_infos[].total_balance | remaining | 保留十进制字符串及币种；包括可用赠金和充值余额 |
| granted_balance / topped_up_balance | 响应结构校验 | 校验数值格式；本期不显示明细，不重复计入余额 |
| is_available | 结构校验 | false 不等于余额字段缺失，也不推断余额为 0 |
| 请求成功时间 | observedAt | 真实成功读取时间，失败不改写 |

余额无上限、无百分比、无重置时间；指标 source=official_api。空列表、缺字段、重复/未知币种和非十进制金额返回 schema_changed，不覆盖旧观测。

接口不提供稳定账号 ID，身份状态为 key_scope_unverified。UI 明确按配置的 Key 绑定，默认暂停；用户确认此 Key 对应目标账号并启用后，仍不标为身份已验证。替换 Key 清除该连接旧观测/历史，避免混用账号。每个 Key 使用独立连接和 credential_ref。

## 失败行为

401：需更新 Key；403：权限不足；429：遵从 Retry-After 秒数或 HTTP 日期，并设置跨账号平台冷却。无提示时至少等 60 秒；超过 7 天的异常 Retry-After 标为字段变化并停止自动查询，避免计时溢出。网络/5xx 按 1/2/5/15/30 分钟退避，手动刷新至少冷却 30 秒；重试时间持久化。

认证/权限/字段变化错误停止自动调度，用户修改连接并重新测试后恢复。失败保留旧数据与观测时间；删除或替换连接后，旧 revision 的在途结果丢弃。

## 测试与真实验证记录

合成 fixture：`fixtures/providers/deepseek-balance.json`，不含账号或凭据。离线测试覆盖双币种与零余额、无百分比、非法地址、缺失字段、HTTP 200/401/403/429/302/500、超时、旧值保留、删除在途结果丢弃和人工记录迁移。

Windows Credential Manager 已使用合成 Key 实测保存、读取、替换、删除，测试凭据清理完成。2026-10-16 用户确认真实账号连接与余额读取成功；余额页面同时间对照未记录，Windows 添加/编辑/双窗口完整交互仍待专项验证。此次反馈不表示全部验证步骤已完成。

## 本机验证步骤

1. `pnpm tauri dev`，点击“添加账号”。
2. 选择 DeepSeek API，填写别名，在专用输入框填 Key（不要粘贴到聊天或备注）。
3. 测试读取，核对币种、金额、时间与平台余额页；确认 Key 所属账号后勾选启用并保存。
4. 等待冷却后刷新，关闭重开验证凭据恢复与余额读取；验证浮窗同步。
5. 替换 Key、停用、删除连接，核对错误与旧历史清理。未通过前保持实验性。
