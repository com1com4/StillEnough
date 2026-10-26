# StillEnough

[简体中文](README.md) | [English](README.en.md)

<p align="center"><img src="public/brand/stillenough.png" width="96" alt="StillEnough Logo" /></p>

**One glance. Keep going.**

An AI quota monitor and API balance tracker for the Windows desktop. Check remaining quotas, reset times, and API wallet balances across multiple providers from one app, with a system tray and desktop widget.

[Download for Windows](https://github.com/com1com4/StillEnough/releases/latest) · [Supported providers](#provider-support) · [Getting started](#getting-started)

## Features

- **Multiple providers in one place**: manage accounts and view subscription quotas, individual reset windows, and API wallet balances.
- **Desktop widget**: pin accounts, drag to reorder, and show 1–8 accounts. Customize aliases, progress bars, reset times, and opacity. Fade while idle, restore on hover, or collapse the widget.
- **Background refresh**: refresh every 10 minutes by default, with 5/10/15/30-minute options. Manual refresh, global pause, and per-account disabling respect provider rate limits.
- **Low-quota alerts and history**: receive in-app subscription quota alerts, view 7/30-day observation charts, and export accounts and current metrics as JSON/CSV.
- **Clear data states**: show quota windows separately and balances by currency. Failed queries retain the previous values and timestamps. Unknown or stale data never becomes a full quota or zero balance.
- **Everyday desktop controls**: system tray, optional startup on login, light/dark/system themes, and global or per-account network settings. API credentials stay in Windows Credential Manager.
- **Chinese and English UI**: choose the system language, Simplified Chinese, or English. The main window, widget, and tray share the locally saved preference.
- **CLI / AI Agent access**: the GUI and CLI ship together. After authorization in the main window, query accounts and quotas or request a background refresh, with JSON output, timeouts, and explicit exit codes.

The current source version is **0.4.3**, in early development. This version adds parsing compatibility for Kimi string counts, multiple GLM quota windows, and Claude model-specific weekly limits; see the [release notes (Chinese)](docs/releases/v0.4.3.md). Client endpoints may change. Validation coverage varies by plan and region, as described below.

## Screenshots

**Main window**: provider quotas, reset times, and API balances, with separate states for updated data, stale data, and paused collection.

![StillEnough main window showing provider quotas, reset times, and API balances](docs/images/overview.png)

**Desktop widget**: keep selected account quotas and balances visible while working.

![StillEnough desktop widget showing MiniMax, Codex, and DeepSeek quotas and balances](docs/images/widget.png)

These screenshots show the running v0.1.0 application.

## Provider support

| Product | Connection | Data queried | Status |
|---|---|---|---|
| Codex · HTTP | Authorized read of local Codex CLI login | ChatGPT subscription quota windows and additional pools | Implemented; successful reads reported by the user |
| Claude Code | Authorized read of local Claude Code login | Subscription quota windows | Implemented; real-account validation pending |
| DeepSeek API | API key | Wallet balances by currency | Implemented; successful reads reported by the user |
| MiniMax API | Regular API key for the selected region | Available amount and cash balance; unknown currency when absent from the source | Implemented; successful reads reported by the user |
| MiniMax Token Plan | Plan key for the selected region | Shared quota pool with 5-hour and weekly windows | Implemented; successful reads reported by the user |
| Alibaba Cloud Bailian API | Alibaba Cloud AccessKey ID / Secret | Cash balance and available credit for the entire Alibaba Cloud account, not a Bailian-specific balance | Implemented · experimental; real-account validation pending |
| Alibaba Cloud Bailian personal Token Plan | Authorized queries through the logged-in official bl CLI | Monthly quota and reset time | Successful reads confirmed; login tokens expire frequently and re-login needs improvement |

“Implemented” means connection setup, test queries, and background refresh are available. User-reported successful reads are based on feedback dated 2026-10-16 and cover only the accounts used, not all plans, regions, or long-running behavior. See [provider documentation (Chinese)](docs/PROVIDERS.md) for sources and validation records.

The Codex app-server adapter remains available for existing accounts; new Codex connections use HTTP. MiniMax international API/Token Plan, GLM China/international Coding Plan, Kimi China/international Code, and Moonshot API balance connections are experimental and await real-account validation. GLM API wallet support is blocked by the lack of a reliable query contract. Gemini and Antigravity are not integrated.

## Getting started

Download the Windows x64 ZIP from [GitHub Releases](https://github.com/com1com4/StillEnough/releases/latest), extract all files, and run `StillEnough.exe`. The desktop app requires Microsoft Edge WebView2 Runtime. You can also run or build from source.

1. Open the app, click “Add account,” and select a product.
2. Configure API credentials locally, or explicitly authorize access to the selected CLI login source.
3. Test the connection and verify the account, quota/balance, and reset time, then enable and save it.
4. Select pinned accounts, widget display options, and network settings.

The app provides no sample accounts or demo data. Before a successful query, values remain unavailable or unknown. Subscription quotas and API wallet balances are separate metrics.

## Running from source

Windows 10/11 x64 is the primary target. Install Node.js, the project's specified pnpm version, Rust stable MSVC, Visual Studio C++ Build Tools, and WebView2.

Dependencies are pinned by `pnpm-lock.yaml` and `Cargo.lock`. The `packageManager` field in `package.json` specifies pnpm 11.9.0. Earlier development used Node.js 22.22.2; later recorded local checks used Node.js 24.21.0 and Rust/Cargo 1.95.0. These are development validation records, not minimum supported versions. Clean Windows setup remains unverified.

Open PowerShell in the cloned repository root:

```powershell
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` starts only the frontend server. Accounts, credentials, and queries require the desktop Rust backend.

## Checks and builds

```powershell
pnpm version:check
pnpm typecheck
pnpm lint
pnpm test
pnpm build
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Frontend tests cover version scripts, history segmentation, presentation, themes, and widget ordering. Rust tests cover parsing, error handling, migrations, alerts, and state management. Real-account behavior and native Windows interactions require separate validation.

Exit the running app from the system tray before building:

```powershell
pnpm desktop:bundle -Bundle none
powershell -NoProfile -File scripts/package-portable.ps1
```

The executables are `target/release/stillenough.exe` and `target/release/stillenough-cli.exe`, distributed together in a portable ZIP. No installer is currently published. `pnpm build` builds only the frontend. See [version management (Chinese)](docs/VERSIONING.md).

## Data and privacy

Account configuration, settings, and observation history are stored in local SQLite. API credentials are stored in Windows Credential Manager. CLI login sources are read only after authorization. StillEnough does not modify other CLIs' login files or use their refresh tokens to renew sessions.

Queries connect to the corresponding provider using the selected network and proxy policy. Exports exclude credentials, authentication paths, and raw responses. Aliases, observations, and timestamps may still contain personal information; review them before sharing.

## Known limitations

- Client endpoints may change or reject third-party access. Test fixtures do not replace real-account validation.
- Default CLI login sources do not provide independent account isolation. Recheck the connection after switching CLI accounts.
- Alerts are currently in-app only; Windows system notifications are not implemented.
- PAC and authenticated proxies are unsupported. The Bailian CLI uses its own environment; see the provider documentation.
- macOS/Linux, clean Windows setup, multiple monitors/DPI settings, sleep recovery, and the updated widget dragging behavior still require validation.

## Documentation and contributing

The following detailed documents are currently in Chinese:

[Product scope](docs/PRODUCT.md) · [Architecture](docs/ARCHITECTURE.md) · [Providers](docs/PROVIDERS.md) · [Current tasks](TASKS.md) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)

When reporting an issue, include the version, product, reproduction steps, and redacted error codes. Do not upload keys, tokens, cookies, authentication files, databases, or unredacted logs.

## License and acknowledgments

Source code is licensed under the [MIT License](LICENSE). Third-party icons and dependencies retain their own licenses; see [third-party notices](THIRD_PARTY_NOTICES.md).

Provider adapters reference official public materials and CodexBar's endpoint research; pinned source commits are documented in the adapter notes. StillEnough is an independent tool and is not an official product of, or affiliated with, the providers.

## CLI / AI Agent

Enable access in the main window under “Settings → CLI / AI Agent access,” then query from PowerShell or an agent:

```powershell
.\stillenough-cli.exe accounts list --json
.\stillenough-cli.exe quota --json
.\stillenough-cli.exe refresh --all --json
.\stillenough-cli.exe doctor --json
```

`version` and `providers` require neither the desktop app nor authorization. Account queries prefer the desktop named pipe; when the endpoint is absent, they fall back only to an authorized read-only cache. Refresh uses the shared desktop backend. The CLI does not read credentials or contact providers directly. It supports `--offline`, `--require-fresh`, and `--timeout`. Stale values are not replenished automatically, and wallet balances are never converted to percentages. See the [CLI guide](docs/CLI.md) and [implementation and validation records](docs/CLI-IMPLEMENTATION.md), both in Chinese.

The GUI and CLI ship in the same portable ZIP and run separately after extraction. Live refresh requires the desktop backend; the CLI does not start a second backend automatically. The CLI itself does not require Node.js or WebView2; the desktop app requires WebView2. VC runtime requirements for both still need clean Windows validation. From source, use `pnpm cli -- quota --json`; `pnpm desktop:bundle -Bundle none` compiles both executables. Standalone CLI ZIP/npm packaging scripts are retained for development only. No public npm package is published, and the system PATH is not modified.
