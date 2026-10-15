# 阿里百炼 API（阿里云账户余额）

2026-10-01：已实现，实验性 / 待真实账号验证。余额属于整个阿里云账号，不能标为百炼专属余额或 Token Plan 额度。

来源：[QueryAccountBalance 官方文档](https://help.aliyun.com/zh/user-center/developer-reference/api-bssopenapi-2017-12-14-queryaccountbalance)、[RPC 签名机制](https://help.aliyun.com/zh/sdk/product-overview/rpc-mechanism)。认证为 AccessKey ID / Secret，RAM 权限 `bss:DescribeAcccount`；模型 `sk-` Key 不适用。

添加账号选择“阿里百炼 API（阿里云账户余额）”，在界面填写两个 AccessKey 字段 → 测试 → 启用 → 保存。仅访问 `https://business.aliyuncs.com/`，GET Action=QueryAccountBalance，Version=2017-12-14，HMAC-SHA1 RPC 签名；随机 nonce、UTC timestamp，签名不记录日志。

| 来源字段 | 显示 |
|---|---|
| Data.AvailableCashAmount | 阿里云账户现金余额 |
| Data.AvailableAmount | 阿里云账户可用额度（含信用） |
| Data.Currency | 按共享 ISO 4217 代码表校验并转大写；缺失或非法代码拒绝 |
| Success / Code | 必须为 true / 200 |

十进制原值，支持零/负余额，不计算百分比。AccessKey 对作为单一密文凭据项保存在 Windows 系统凭据库，SQLite 只保存不透明引用；原始响应/请求不进入 UI 或导出。接口未核验账号身份，用户需在官方费用控制台确认账号。

合成 fixture `bailian-balance.json` 覆盖零现金和不同可用额度；解析、域名、HTTP 失败测试通过。真实权限、签名与官方服务交互、币种/金额对照、Windows UI 尚未实测。测试前不能标为已验证；失败保留历史数据及时间。
