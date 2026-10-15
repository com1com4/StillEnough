# 贡献指南

请先阅读 README、AGENTS.md、docs/ARCHITECTURE.md 与 docs/PROVIDERS.md。

按 README 在 Windows PowerShell 安装依赖并启动桌面。提交依赖变更同步锁文件，数据库变更新增迁移，不重写已有迁移。

UI 不直接请求厂商、读取凭据或访问 SQLite；适配器集中注册，未知不造数，余额不冒充额度。不加入示例账号、运行时 mock 回退、浏览器凭据提取、自动续费或自动模型路由。

## 验证与提交

说明问题、最终行为、实际检查和未验证项。前端业务改动运行 typecheck/lint/test/build，Rust 改动运行 fmt/check/clippy/test。纯文案/样式按风险检查，不堆砌测试。编译通过不等于 Windows 或真实账号验收通过。

新增适配器附官方/固定提交来源、字段语义、合成 fixture、解析和失败测试；无真实账号证据标实验性。

检查差异及新增文件，排除认证文件、SQLite、日志、导出、截图中的个人信息和本地生成物。不提交个人绝对路径或代理配置。

## 问题报告

提供应用/Windows 版本、产品与连接方式、复现步骤、预期/实际行为、脱敏错误码。截图只保留必要部分；不得上传 Key、Token、Cookie、认证文件或数据库。安全问题见 SECURITY.md。
