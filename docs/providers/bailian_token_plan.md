# 阿里百炼个人 Token Plan

更新：2026-10-16。实验性；通过用户显式授权的官方 bl CLI 查询当前个人 Token Plan，不属于 API 钱包余额或团队席位支持。

## 连接

先按官方 CLI 文档安装并登录，在应用添加百炼个人 Token Plan → 授权官方 CLI → 测试 → 核对账号并启用保存。登录由官方工具完成，本应用不读取/改写其认证文件，不要求输入临时令牌。

检测读取进程和 Windows 用户/系统 PATH，支持 bl/bailian 二进制与 npm 入口。cmd/ps1 标记仅用于定位，不执行 shell shim；按包名/bin 声明解析并限制包内路径，优先同目录 Node。开发工作区/本应用目录排除，避免执行项目同名程序。

授权后固定版本/帮助探测，单次限时、整轮探测有限时；输出不回传前端或持久化。兼容性检测不等于发行物签名认证。查询使用隐藏子进程、输出长度上限和超时。

## 查询与字段

固定 usage token-plan 查询个人套餐；官方 CLI 2.1.0 的便捷命令仅挑选四个旧字段，月额度响应可能输出空对象。遇到已知无额度结构时，通过官方 CLI 固定 console call 请求同一 tokenplan/personal/api/v2/usage 端点，不切换 Coding Plan，不直接访问认证文件。

per1MonthPercentage 是已用比例 0–1，剩余=(1-ratio)×100；per1MonthResetTime 是来源重置时间。兼容旧 5 小时/周字段，只有月字段时只展示月窗口，不凑齐缺失窗口。网关和内层 success 均需校验，空已知结果报 cli_quota_unavailable，未知结构报 schema_changed，均不能作为读取成功。

CLI 仅沿用自身环境，不支持应用显式直连/自定义代理覆盖；可对该账号选择跟随系统。默认登录来源不具备多账号隔离，切换官方 CLI 登录后需重新测试。

## 验证

历史记录中官方 CLI 2.1.0 的空结果原因已定位，后台月额度真实读取通过。2026-10-16 用户确认已连接并成功读取，但反馈登录 Token 容易过期、重新登录体验较差；这是登录有效期问题，不是查询请求超时。具体有效期及过期提示流程尚未专项验证，不推定原因或时长。官方控制台同时间数值/重置对照和 Windows 完整表单/浮窗流程待验。合成 fixture 仅用于解析证据。

2026-10-16 发布检查：沙箱内 child_success_failure_timeout_and_output_limit 曾报 cli_query_failed；沙箱外单项及完整离线测试复核通过（90 通过、0 失败、7 跳过）。合成子进程测试结果不能代替真实登录有效期或平台稳定性验证。

迁移 004 将旧令牌连接暂停、安排旧凭据清理并保留历史，不默认授予 CLI 权限。

## 来源

- [官方认证说明固定提交](https://github.com/modelstudioai/cli/blob/8bbbbc722d70fb200641ef22b6f6d033aeae9f74/skills/bailian-cli/reference/auth.md)
- [官方用量命令固定提交](https://github.com/modelstudioai/cli/blob/8bbbbc722d70fb200641ef22b6f6d033aeae9f74/packages/commands/src/commands/usage/token-plan.ts)
- [官方 npm 入口固定提交](https://github.com/modelstudioai/cli/blob/8bbbbc722d70fb200641ef22b6f6d033aeae9f74/packages/cli/package.json)
- [个人 Token Plan FAQ](https://help.aliyun.com/zh/model-studio/token-plan-personal-faq)
