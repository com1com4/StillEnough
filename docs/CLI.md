# CLI 使用说明

CLI 查询已由桌面配置的真实账号。先启动匹配版本 StillEnough，在主窗口“设置 → CLI / AI Agent 访问”开启访问权限；触发刷新和桌面关闭后的缓存读取各有独立开关，默认全部关闭。此开关独立于每连接的厂商 CLI 登录来源授权。CLI 没有 grant、登录或导入凭据命令。

## 安装与运行

源码（PowerShell，无需 bash）：

```powershell
pnpm cli -- version --json
pnpm cli -- providers
pnpm cli -- accounts list --json
pnpm cli -- quota --json
pnpm cli:build
cargo install --path ./crates/stillenough-cli --locked
```

正式发布的 Windows x64 便携 ZIP 同时包含 `StillEnough.exe` 和 `stillenough-cli.exe`，只需下载、解压一次。CLI 不需要 Node 或 WebView2，桌面后台需要 WebView2。本机 release 二进制通过依赖检查确认仍需要 Windows VC 运行库（VCRUNTIME140.dll），干净系统需另行核查该依赖。ZIP 不会修改 PATH。含空格路径用 PowerShell 调用运算符：

```powershell
& 'C:\Tools\StillEnough CLI\stillenough-cli.exe' quota --json
```

本地 npm wrapper 为准备发布的包，不表示已发布到公共 registry。本地安装需要同时提供 wrapper 与 Windows 平台包，安装不会运行 postinstall 或下载二进制。npm 渠道需要 Node 22+：

```powershell
npm install --prefix ./cli-local --ignore-scripts ./stillenough-cli-0.2.0.tgz ./stillenough-cli-win32-x64-0.2.0.tgz
& ./cli-local/node_modules/.bin/stillenough-cli.cmd version --json
```

## 命令

| 命令 | 用途 |
|---|---|
| version | CLI/core/协议版本，本地无需授权 |
| providers [--provider ID] | 八个 Provider 的静态支持范围；不表示账号已配置或已实测 |
| accounts list [--provider ID] [--offline] | 账号清单，不请求指标 |
| quota [--account ID 或 --provider ID] [--offline] [--require-fresh] | 缓存快照，不隐式联网 |
| refresh (--account ID 或 --all [--provider ID]) | 通过桌面唯一后台刷新，默认等待持久化 |
| operations get ID | 读取刷新操作结果，后台重启后返回 operation_lost |
| doctor [--account ID] [--offline] | 协议/schema/权限/已知账号状态；不读取凭据 |
| doctor --account ID --live | 通过同一刷新服务进行诊断并保存结果，需要刷新授权 |

全命令支持 `--json`、`--timeout 1–300`（默认 30 秒）、`--no-color`。默认是制表符表格，无交互、分页或颜色；`--help`/`--version` 为文本元命令。账号 ID 来自 accounts list，不使用别名定位。

JSON 是 UTF-8 无 BOM，一个完整对象加换行；消费者先保存退出码，再解析：

```powershell
$result = & ./target/release/stillenough-cli.exe quota --json
$exitCode = $LASTEXITCODE
$data = $result | ConvertFrom-Json
```

字段遵循 [Schema v1](cli-response-v1.schema.json)。账号 status 与 metric freshness 分开；钱包百分比恒 null，未知值保持 null，真实零保留为零。旧观测保留原时间，到 TTL/重置边界只标 expired，不补成 100%。--require-fresh 不满足时仍保留旧快照，退出 11。

默认查询先走当前用户、当前 Windows 会话的命名管道。只有端点不存在时才尝试已授权只读缓存；拒绝授权、身份不匹配、协议不兼容和断管道不会自动回退。--offline 只读同一用户数据目录中的 SQLite，无迁移、建库或凭据读取；离线无运行态 revision/instanceId，warnings 标注 runtime_unavailable。

刷新与桌面共用全局并发 3、每账号 gate、30 秒防抖、平台冷却、revision 和提醒去重；不支持 --force。显式手动刷新可以在全局自动采集暂停时执行（返回 collection_paused 提示），停用账号不刷新。请求超时可能返回 operationId，后台工作不保证被取消；用 operations get 查询。权限撤销阻止新任务与尚未发出的排队请求，正在进行的网络请求无法收回，结果不再交给被撤销的请求。操作记录仅存在当前实例，最多 128 项，新刷新到来时清理创建超过一小时的已完成记录；同 requestId 在保留期内去重。

## 退出码

| 码 | 含义 |
|---|---|
| 0 | 读取/执行成功；仍需检查账号与指标状态 |
| 2 | 参数错误 |
| 3 | 无桌面、未初始化或缓存不可用 |
| 4 | Agent/来源授权或认证不满足 |
| 5 | 厂商拒绝、接口变化、套餐/CLI 不兼容 |
| 6 | 限流、防抖、忙、账号停用、数据库忙 |
| 7 | 超时/网络错误 |
| 8 | 批量部分失败 |
| 9 | 协议/schema/所有者不兼容 |
| 10 | 内部/存储故障 |
| 11 | require-fresh 不满足 |
| 12 | 对象或操作不存在 |

只支持本机当前交互用户/会话，提权与非提权客户端不互访。不接受任意 pipe、数据库、认证文件或可执行路径。管道 DACL 拒绝远程用户，客户端与服务端核对 SID、会话及提权状态；本工具不隔离同用户恶意程序。离线权限依赖同用户文件 ACL，拥有同用户写权限的程序可修改本地库，不能把它当作 OS 隔离方案。

升级桌面前退出旧版，旧版没有新所有者锁。新 host 在开库前获取数据目录范围的 Windows 内核对象锁，持有到进程退出；SQLite 设置 WAL 和 5 秒 busy_timeout，启动前检查完整性、拒绝未来 schema，需迁移时先做一致性备份。CLI 不启动第二后台、不执行迁移、不删除 WAL/shm。

## 本地开发打包

正式发布仅提供包含 GUI 和 CLI 的便携 ZIP 与 SHA256 校验文件，不发布安装程序、独立 CLI ZIP 或 npm 包。以下脚本仅用于开发阶段生成额外包，不是当前发布流程。

```powershell
powershell -NoProfile -File scripts/package-cli.ps1
# 同时生成包含桌面与 CLI 的 ZIP：
powershell -NoProfile -File scripts/package-cli.ps1 -IncludeDesktop
```

产物位于 output/cli-release/v<版本>，包含 ZIP、本地 npm tgz 和 SHA256SUMS.txt。脚本拒绝覆盖已有版本目录。签名、公共 npm 包名归属和对外发布均需另行办理。验证状态见 [实施记录](CLI-IMPLEMENTATION.md)。

PowerShell 5.1 原生管道捕获建议先在当前进程设置 `[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)` 和 `$OutputEncoding = [Console]::OutputEncoding`；其 `>` 默认会重编码为 UTF-16，若需要 UTF-8 文件，使用 `[IO.File]::WriteAllText` 明确 UTF-8 无 BOM。PS7 的原生重定向已通过检查。传输超时若附 operation_state_unknown，操作 ID 是查询线索，不代表后台已确认开始。

便携版构建使用 `pnpm desktop:bundle -Bundle none`，两个 EXE 位于 `target/release/`。`pnpm desktop:bundle` 默认仍可生成含 CLI sidecar 的 NSIS，供开发使用，当前不对外发布；未完成安装/卸载验收。
