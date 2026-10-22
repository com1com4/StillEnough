# 当前待办与验证

## v0.4.2 发布检查（2026-10-22）

- [x] GUI、CLI、workspace 和 Cargo.lock 同步为 0.4.2，包含浮窗焦点框修复；说明见 docs/releases/v0.4.2.md。
- [x] 版本一致性、版本脚本测试 1 项、前端测试 45 项、TypeScript、ESLint、Vite 构建与 git diff --check 通过，直接调用已安装工具。
- [x] GUI/CLI release 构建、便携 ZIP 双端版本、x64 架构、8 文件白名单、逐文件哈希与 SHA256SUMS 通过。首次构建因运行中的目标 EXE 占用失败，关闭该实例后重建成功。
- [ ] 原生焦点切换、拖动、Tab 导航复现与干净 Windows 仍待验证；未新增真实账号验证，本次未重复完整 Rust 测试。

## 浮窗焦点框裁切修复（2026-10-22）

- [x] 账号行单独使用向内绘制的细焦点框，避免继承全局外扩焦点框后被滚动容器裁切成跨行蓝线；保留键盘焦点提示。
- [x] 直接调用已安装工具：TypeScript、ESLint、Vite 前端构建及 git diff --check 通过。
- [ ] Windows 原生复现与确认：窗口重新显示、切换焦点、Tab 切换账号、拖动后恢复。当前依据截图和 CSS 定位，未完成原生验证。

运行：pnpm tauri dev；下一步确认上述操作后不再出现孤立横线。

更新：2026-10-17。实现与真实账号/Windows 验收分开。源码运行与检查命令见 README。

## 已实现范围

- [x] Tauri/Rust/React、SQLite 001–006 迁移、系统凭据封装。
- [x] API/CLI 连接、测试、保存、刷新与采集；无人工额度/演示模式。
- [x] 共享后台、托盘、浮窗、主题、代理与账号覆盖。
- [x] 应用内提醒、历史、导出和本工具数据清理。
- [x] 浮窗显示选项与关注账号排序/拖拽实现，原生交互另验。
- [x] 版本脚本、关于页、隐藏 app-server 新增入口。
- [x] 平台范围集中记录到 docs/PROVIDERS.md。
- [x] 简体中文 / English 语言设置、后台持久化、主窗口/浮窗/托盘文案及 Provider 说明；自动验证记录见 [多语言实现](docs/LOCALIZATION.md)。
- [x] 主窗口固定左侧导航及标题栏、右侧内容独立滚动、切页返回顶部和主题滚动条样式。
- [x] 版本元信息统一升级到 0.2.0；变更及验证范围见 [发布说明](docs/releases/v0.2.0.md)。

## 真实平台

- [x] Codex HTTP、DeepSeek API、MiniMax API/Token Plan 真实账号连接与读取：2026-10-16 用户确认成功，具体套餐/地区覆盖未记录。
- [ ] Codex HTTP 完整桌面流程、自动刷新/重启、官方页面窗口/重置对照。
- [ ] Claude 真实订阅、权限、过期及计划差异。
- [ ] DeepSeek、MiniMax API/Token Plan 币种/区域/权限与页面对照；百炼 API 真实账号读取待验证。
- [x] 百炼个人 Token Plan 真实读取：已有后台记录，2026-10-16 用户确认成功。
- [ ] 百炼个人 Token Plan 登录 Token 容易过期、重新登录体验较差：2026-10-16 用户反馈，具体有效期及过期处理待排查，不是查询请求超时。
- [ ] 百炼个人 Token Plan 控制台数值/重置对照、干净 Windows CLI 检测。
- [ ] CLI 切换身份后的连接核对。

## Windows 与发布

- [ ] 多语言原生实测：主窗口切换后浮窗与托盘同步、重启恢复、长英文提示、8 账号和高 DPI 布局；实现与自动检查见 [多语言实现](docs/LOCALIZATION.md)。

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

## CLI P1（2026-10-18）

- [x] 根 workspace、最小 core/CLI 边界、共享 Provider 描述投影、DTO/状态枚举；version/providers 本地表格与 JSON 命令。
- [x] 实际检查与基线比较见 [CLI P1 记录](docs/CLI-P1.md)；pnpm 三项在沙箱外使用工作区 TEMP 后通过，Rust fmt/check/clippy 通过。
- [x] 后续完整 Rust 复核：普通 Windows 用户环境全部通过（7 项真实环境专项仍忽略）；P1 历史失败保留在 CLI-P1.md，不覆盖历史结果。
- [x] P2–P4：离线查询与 main-only UI 授权、严格 DACL Windows 管道、共用后台刷新/操作/诊断；自动化和 OS 管道证据见 CLI-IMPLEMENTATION.md。
- [x] P5 本地分发工程：CLI scripts、cargo install、ZIP、npm wrapper/平台包、桌面 ZIP/NSIS sidecar 构建入口、使用文档。
- [ ] P5 环境验收：干净 Windows 无 Node/WebView2、完整 NSIS 安装卸载/签名、提权/跨会话/远程对抗、远端 CI（本次未配置）；未发布公共 npm 包。

## v0.3.0 便携发布（2026-10-18）

- [x] 同步 workspace、GUI、CLI 与锁文件版本为 0.3.0，包含设置页圆形滑块开关；README 与 CLI 文档统一为 GUI/CLI 同包便携发布。
- [x] 普通 Windows 用户环境：pnpm test（版本脚本 1 项、前端 42 项）、lint、typecheck；Rust fmt/check/clippy/test 工作区检查通过（122 通过、7 忽略）。历史失败记录保留，不以忽略的真实环境专项作为验证通过。
- [x] JSON Schema 2020-12、秘密哨兵、参数错误退出码与 Unicode 输出检查通过。
- [x] v0.3.0 GUI/CLI release 构建通过；便携 ZIP 白名单（8 个文件）、x64 架构、两端版本、逐文件哈希通过。打包后的原生桌面启动有非零窗口句柄，CLI 通过 IPC 返回默认未授权/退出 4；Unicode 与 UTF-8 无 BOM 重定向通过。未开启访问权限、未触发真实账号刷新，测试进程已关闭。
- [ ] 干净 Windows、真实账号范围、设置开关原生交互、多屏/DPI、长期运行仍待验证；本版不发布安装程序、独立 CLI ZIP 或 npm 包。

## 地区 Provider 扩展（2026-10-18）

- [x] MiniMax 海外 API/Token Plan、GLM 国内/海外 Coding Plan、Kimi 国内/海外 Code：共享注册表、桌面适配、CLI 描述、双语连接说明和实验性标识。
- [x] 合成 fixture 与额度方向、耗尽、缺失、非法数值、日期和官方地址白名单测试已编写；来源见 docs/PROVIDERS.md。
- [ ] 新地区真实账号读取、官方页面数值/重置对照和 Windows 原生添加/刷新流程。

- [x] Kimi / Moonshot 国内、海外 API 余额独立入口及负余额、币种与缺字段合成测试。
- [ ] GLM 国内/海外 API 钱包：缺少可靠官方查询契约，详见 docs/PROVIDERS.md blocker。

本次验证：普通 Windows 用户环境（2026-10-18），pnpm typecheck/lint/test/build 通过（版本脚本 1 项、前端 42 项）；cargo fmt/check（clippy 隐含编译）/clippy --workspace --all-targets --locked -- -D warnings 通过；cargo test --workspace --locked -- --test-threads=1 通过，共 126 项、7 项真实环境专项忽略。最初沙箱内 pnpm/TEMP/命名管道失败；工作区 TEMP 下并行 Rust 检查也发生 CLI 发现及网络子进程失败，改为默认系统 TEMP、普通用户环境串行复核后全部通过。未查询真实账号，未进行原生添加/刷新验收，未构建发布安装包。

运行：pnpm tauri dev（桌面），pnpm dev（仅前端预览）；设置连接时按产品和地区选择独立入口，输入对应 API 或套餐 Key 并测试连接。下一步为新增地区账号与官方页面对照；GLM 钱包继续等待可靠接口契约。

## 浮窗闲置淡化（2026-10-19）

- [x] 设置增加“闲置时淡化”，默认开启；整体以可设置的闲置不透明度显示（1–100%，默认 10%），悬停或键盘聚焦恢复用户设置的透明度，移出后 180ms 淡化；减少动态效果偏好下不播放过渡。折叠状态同样生效。
- [x] 复用后台 widgetDisplay 设置保存与跨窗口同步；旧设置补全默认值，关闭开关后保持原有固定透明度。
- [ ] Windows 原生实测：悬停/移出、拖动后移出、右键菜单、折叠、重启恢复与多屏/DPI。视觉淡化不改变浮窗尺寸或鼠标命中范围。

运行：pnpm tauri dev，设置页“闲置时淡化”开关；仅前端预览为 pnpm dev。

检查：直接调用已安装工具，TypeScript、ESLint、Vite 构建、前端 42 项及版本脚本 1 项通过；cargo fmt 与 widget_display_tests 2 项通过；git diff --check 通过。pnpm 启动器锁文件写入受限，Vitest 首次系统 TEMP 失败，改用工作区 TEMP 后通过。Windows 原生视觉交互尚未实测。

闲置不透明度可调补充：1–100%，默认 10%，后台保存及边界校验；直接调用已安装工具的 TypeScript、ESLint、前端 42 项和 Vite 构建通过，Rust 设置测试 3 项通过（首次新增测试缺少必填字段，补齐后通过）。原生悬停与滑块交互仍待实测。

## 浮窗操作区调整（2026-10-19）

- [x] 移除覆盖第一行额度的悬停按钮组；悬停仅恢复不透明度。
- [x] 取消底部操作条和重复菜单按钮；操作统一使用原生右键菜单，保留 Escape 隐藏。
- [x] 平台图标/名称/别名所在左侧区域复用为拖动区，不增加高度；其子元素不拦截拖动命中，点击该区不打开主窗口。右侧额度仍可点击打开主窗口，空状态标题支持拖动。只有账号数超出上限时保留“更多账号”入口。
- [ ] Windows 原生实测：平台区域拖动及右键、拖动后不误开主窗口、折叠、空状态、8 账号与闲置淡化；构建不能替代桌面验收。

## v0.4.0 发布检查（2026-10-19）

- [x] 浮窗紧凑交互提交为 8f577a2；版本脚本同步 GUI/CLI/workspace/锁文件到 0.4.0。
- [x] 版本脚本测试、TypeScript、ESLint、前端 42 项和 Vite 构建通过（直接调用已安装工具）；普通 Windows 用户环境 cargo fmt、clippy --workspace --all-targets --locked -- -D warnings、cargo test --workspace --locked -- --test-threads=1 通过，7 项真实环境专项忽略。
- [ ] 原生悬停、平台区域拖动、右键菜单、多屏/DPI、干净 Windows 与新增地区真实账号仍待验证；不以发布构建代表桌面交互验收。

- [x] v0.4.0 GUI/CLI release 构建通过；便携 ZIP 的两端版本、x64 架构、8 文件白名单、逐文件哈希及 SHA256SUMS 生成通过。未启动新版操作真实账号，未进行原生交互验收。

## 产品选择面板（2026-10-20）

- [x] 添加账号以平台列表、产品选项和地区按钮替代长下拉；支持平台/产品/地区关键词搜索、无结果提示、当前选择摘要和分步配置。平台与产品区域分别滚动，支持深浅主题与中英文。
- [x] 产品和地区映射集中在 providerPicker.ts；只展示后台已注册入口，未来未配置展示元数据的产品仍可显示。不生成演示账号或指标。
- [x] 选择后进入原连接配置、测试和保存流程；可返回更换产品，更换会清除测试结果及凭据输入。已有账号直接进入配置，产品保持固定；app-server 继续不出现在新增入口。
- [x] 直接调用已安装工具：TypeScript、ESLint、Vite 构建、前端 45 项测试通过；测试初次系统 TEMP 读取失败，工作区 TEMP 复核通过。新增测试覆盖地区分组、套餐与钱包隔离、组合搜索及未知产品回退。
- [ ] Windows 原生实测：搜索/切换地区/更换产品后的完整测试保存、低高度窗口和高 DPI 滚动、中英文与键盘操作。未查询真实账号，未构建发布包。

运行：pnpm tauri dev，点击“添加账号”；pnpm dev 仅提供前端页面，不含桌面 Provider 后台。

## v0.4.1 发布检查（2026-10-20）

- [x] GUI、CLI、workspace 和 Cargo.lock 版本同步到 0.4.1；包含本次产品选择面板调整，发布说明见 docs/releases/v0.4.1.md。
- [x] 直接调用已安装工具：版本一致性、版本脚本测试 1 项、TypeScript、ESLint、前端测试 45 项、Vite 构建通过；未验证干净安装或 pnpm 启动器。
- [x] Rust fmt、工作区 Clippy（含编译检查）通过；完整测试在普通 Windows 用户环境 127 项通过、7 项真实环境专项忽略。沙箱内命名管道/CLI 子进程用例失败，普通环境复核通过，未跳过失败用例。
- [ ] 新面板 Windows 原生交互、完整真实账号连接/保存、键盘、低高度窗口与高 DPI 仍待验证；本版未增加真实账号实测范围。

- [x] GUI/CLI release 构建通过；便携 ZIP 双端版本、x64 架构、8 文件白名单、逐文件哈希和 SHA256SUMS 生成通过。构建使用已安装 Node/Tauri 工具及临时 beforeBuildCommand 配置，不修改正式 pnpm 配置。未启动新版操作真实账号。
