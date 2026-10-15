# 当前待办与验证

更新：2026-10-16。实现与真实账号/Windows 验收分开。源码运行与检查命令见 README。

## 已实现范围

- [x] Tauri/Rust/React、SQLite 001–006 迁移、系统凭据封装。
- [x] API/CLI 连接、测试、保存、刷新与采集；无人工额度/演示模式。
- [x] 共享后台、托盘、浮窗、主题、代理与账号覆盖。
- [x] 应用内提醒、历史、导出和本工具数据清理。
- [x] 浮窗显示选项与关注账号排序/拖拽实现，原生交互另验。
- [x] 版本脚本、关于页、隐藏 app-server 新增入口。
- [x] 平台范围集中记录到 docs/PROVIDERS.md。

## 真实平台

- [ ] Codex HTTP 完整桌面流程、自动刷新/重启、官方页面窗口/重置对照；已有一次真实只读查询。
- [ ] Claude 真实订阅、权限、过期及计划差异。
- [ ] DeepSeek、MiniMax API/Token Plan、百炼 API 真实账号、币种/区域/权限与页面对照。
- [ ] 百炼个人 Token Plan 控制台数值/重置对照、干净 Windows CLI 检测；已有真实后台月额度读取。
- [ ] CLI 切换身份后的连接核对。

## Windows 与发布

- [ ] 新浮窗排序/拖拽、同步、重启恢复、长别名、8 账号。
- [ ] 关于页、添加入口及已有 app-server 管理。
- [ ] 多屏/DPI、睡眠恢复、代理变化。
- [ ] 干净 Windows 安装依赖、构建、NSIS 安装卸载、WebView2 与开机启动。
- [ ] macOS/Linux 兼容，当前不承诺支持。
- [ ] Windows CI、首个公开 Release 安装包。
- [ ] 自动测试复核，特别是百炼 CLI 子进程用例的环境稳定性。

## 验证限制

2026-10-15 历史记录中，Rust 用例 child_success_failure_timeout_and_output_limit 失败；不能把过去检查描述为全部通过。后续检查结果另记。

本机 pnpm 存在 ERR_PNPM_LOCKFILE_WRITE_FILE 权限问题；直接执行已安装工具不等于指定 pnpm 的干净安装已通过。真实凭据专项不默认执行。

## 开源准备

- [x] 公开 README、当前产品/架构/平台说明、贡献、安全与 MIT 许可。
- [x] 原开发历史与未提交改动保留，按当前目录导出发布副本。
- [x] 发布副本清单/哈希/链接检查与自动检查结果已记录，Rust 有一个已有失败项，见 docs/RELEASE_CHECKLIST.md。
- [ ] 独立副本首次提交与 GitHub 推送，不推送旧开发历史。
