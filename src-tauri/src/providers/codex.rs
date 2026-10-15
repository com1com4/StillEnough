use super::{FetchError, FetchFuture, Provider};
use crate::{
    domain::models::{ProviderDescriptor, QuotaMetric},
    services::network::NetworkConfig,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub struct CodexProvider;
pub const SOURCE: &str = "cli://codex/subscription";
impl Provider for CodexProvider {
    fn requires_credential(&self) -> bool {
        false
    }
    fn network_kind(&self) -> &'static str {
        "cli_env"
    }
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor { id: "codex", name: "Codex · app-server", supported: true, auth_method: "cli", base_urls: vec![SOURCE],
            capability: "ChatGPT 订阅额度 · 客户端实际返回的窗口", status: "实验性 / 待真实账号验证",
            limitation: "使用本机 Codex app-server 的只读 account/read 和 account/rateLimits/read。不读取 auth.json、不主动刷新令牌、不启动模型任务。仅支持 ChatGPT 订阅登录，同一来源限一个账号；切换登录后请重新测试。",
            setup_instructions: "先安装并使用官方 Codex CLI 登录 ChatGPT 账号：\nnpm install -g @openai/codex\ncodex login\n尊重 CODEX_HOME。仅安装桌面版而没有 CLI 时需另行安装 CLI。",
            authorization_label: "我授权应用启动本机 Codex app-server，只读查询当前登录账号的订阅额度",
            network_note: "支持直连、自定义 HTTP 代理及系统环境/Windows 静态代理。Codex 子进程暂不支持应用指定的 SOCKS 代理。" }
    }
    fn validate_url(&self, url: &str) -> Result<(), String> {
        if url == SOURCE {
            Ok(())
        } else {
            Err("请选择 Codex 本地 CLI 来源".into())
        }
    }
    fn fetch<'a>(&'a self, _: &'a reqwest::Client, url: &'a str, _: &'a str) -> FetchFuture<'a> {
        Box::pin(async move {
            self.validate_url(url)
                .map_err(|_| FetchError::new("unsupported"))?;
            query(&NetworkConfig::system()).await
        })
    }
    fn fetch_routed<'a>(
        &'a self,
        _: &'a reqwest::Client,
        url: &'a str,
        _: &'a str,
        network: &'a NetworkConfig,
    ) -> FetchFuture<'a> {
        Box::pin(async move {
            self.validate_url(url)
                .map_err(|_| FetchError::new("unsupported"))?;
            query(network).await
        })
    }
}

fn discover_in(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for directory in paths {
        let mut possible = vec![directory.join("codex.exe")];
        let package = directory.join("node_modules/@openai/codex");
        if package.join("package.json").is_file() {
            let identity = std::fs::File::open(package.join("package.json"))
                .ok()
                .and_then(|f| {
                    serde_json::from_reader::<_, Value>(std::io::Read::take(f, 65536)).ok()
                });
            if identity.as_ref().and_then(|v| v["name"].as_str()) == Some("@openai/codex") {
                let (platform, triple) = if cfg!(target_arch = "aarch64") {
                    ("codex-win32-arm64", "aarch64-pc-windows-msvc")
                } else {
                    ("codex-win32-x64", "x86_64-pc-windows-msvc")
                };
                for root in [
                    package.join(format!("node_modules/@openai/{platform}/vendor")),
                    directory.join(format!("node_modules/@openai/{platform}/vendor")),
                    package.join("vendor"),
                ] {
                    possible.push(root.join(triple).join("bin/codex.exe"));
                }
            }
        }
        for path in possible {
            if let Some(path) = super::bailian_cli::trusted(&path).filter(|p| p.is_file()) {
                if !candidates.contains(&path) {
                    candidates.push(path);
                }
            }
        }
        if candidates.len() >= 8 {
            break;
        }
    }
    candidates
}
async fn discover() -> Result<PathBuf, FetchError> {
    let candidates =
        tauri::async_runtime::spawn_blocking(|| discover_in(&super::bailian_cli::search_paths()))
            .await
            .map_err(|_| FetchError::new("codex_cli_not_found"))?;
    if candidates.is_empty() {
        return Err(FetchError::new("codex_cli_not_found"));
    }
    for path in candidates {
        let mut command = tokio::process::Command::new(&path);
        command
            .arg("--version")
            .current_dir(path.parent().unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let Ok(mut child) = command.spawn() else {
            continue;
        };
        let Some(stdout) = child.stdout.take() else {
            continue;
        };
        let probe = tokio::time::timeout(Duration::from_secs(3), async {
            let mut bytes = Vec::new();
            stdout.take(4097).read_to_end(&mut bytes).await.ok()?;
            let status = child.wait().await.ok()?;
            if status.success()
                && bytes.len() <= 4096
                && String::from_utf8_lossy(&bytes)
                    .trim()
                    .starts_with("codex-cli ")
            {
                Some(())
            } else {
                None
            }
        })
        .await;
        if matches!(probe, Ok(Some(()))) {
            return Ok(path);
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    Err(FetchError::new("codex_cli_incompatible"))
}

fn configure_network(
    command: &mut tokio::process::Command,
    config: &NetworkConfig,
) -> Result<(), FetchError> {
    config
        .validate(true)
        .map_err(|_| FetchError::new("proxy_config_invalid"))?;
    match config.mode.as_str() {
        "direct" | "custom" => {
            for key in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "NO_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
                "no_proxy",
            ] {
                command.env_remove(key);
            }
            if config.mode == "custom" {
                if !config.proxy_url.starts_with("http://") {
                    return Err(FetchError::new("cli_proxy_unsupported"));
                }
                command
                    .env("HTTP_PROXY", &config.proxy_url)
                    .env("HTTPS_PROXY", &config.proxy_url);
            }
        }
        "system" => {
            #[cfg(windows)]
            {
                if ![
                    "HTTP_PROXY",
                    "HTTPS_PROXY",
                    "ALL_PROXY",
                    "http_proxy",
                    "https_proxy",
                    "all_proxy",
                ]
                .iter()
                .any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
                {
                    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
                    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(
                        "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
                    ) {
                        let enabled: u32 = key.get_value("ProxyEnable").unwrap_or_default();
                        let pac: String = key.get_value("AutoConfigURL").unwrap_or_default();
                        if enabled == 0 && !pac.is_empty() {
                            return Err(FetchError::new("cli_proxy_unsupported"));
                        }
                        if enabled != 0 {
                            let proxy: String = key
                                .get_value("ProxyServer")
                                .map_err(|_| FetchError::new("proxy_config_invalid"))?;
                            for (scheme, env) in [("http", "HTTP_PROXY"), ("https", "HTTPS_PROXY")]
                            {
                                let selected = if proxy.contains('=') {
                                    proxy
                                        .split(';')
                                        .find_map(|p| p.trim().strip_prefix(&format!("{scheme}=")))
                                } else {
                                    Some(proxy.as_str())
                                };
                                if let Some(selected) = selected {
                                    let value = if selected.contains("://") {
                                        selected.to_string()
                                    } else {
                                        format!("http://{selected}")
                                    };
                                    if !value.starts_with("http://") {
                                        return Err(FetchError::new("cli_proxy_unsupported"));
                                    }
                                    command.env(env, value);
                                }
                            }
                            let bypass: String = key.get_value("ProxyOverride").unwrap_or_default();
                            command.env(
                                "NO_PROXY",
                                bypass
                                    .split(';')
                                    .filter(|p| *p != "<local>")
                                    .collect::<Vec<_>>()
                                    .join(","),
                            );
                        }
                    }
                }
            }
        }
        _ => return Err(FetchError::new("proxy_config_invalid")),
    }
    Ok(())
}

type Lines = tokio::io::Lines<BufReader<tokio::io::Take<tokio::process::ChildStdout>>>;
async fn rpc(
    input: &mut tokio::process::ChildStdin,
    lines: &mut Lines,
    id: i64,
    method: &str,
    params: Option<Value>,
) -> Result<Value, FetchError> {
    let mut request = json!({"id":id,"method":method});
    if let Some(params) = params {
        request["params"] = params;
    }
    input
        .write_all(format!("{request}\n").as_bytes())
        .await
        .map_err(|_| FetchError::new("cli_query_failed"))?;
    for _ in 0..128 {
        let line = lines
            .next_line()
            .await
            .map_err(|_| FetchError::new("cli_query_failed"))?
            .ok_or_else(|| FetchError::new("cli_query_failed"))?;
        let frame: Value =
            serde_json::from_str(&line).map_err(|_| FetchError::new("schema_changed"))?;
        if frame.get("method").is_some() {
            if let Some(server_id) = frame.get("id") {
                input.write_all(format!("{}\n",json!({"id":server_id,"error":{"code":-32601,"message":"Read-only client"}})).as_bytes()).await.map_err(|_|FetchError::new("cli_query_failed"))?;
            }
            continue;
        }
        if frame["id"].as_i64() != Some(id) {
            continue;
        }
        if let Some(error) = frame.get("error") {
            return Err(rpc_error(error));
        }
        return frame
            .get("result")
            .cloned()
            .ok_or_else(|| FetchError::new("schema_changed"));
    }
    Err(FetchError::new("schema_changed"))
}
fn rpc_error(error: &Value) -> FetchError {
    let text = error["message"].as_str().unwrap_or("").to_lowercase();
    let code = if error["code"].as_i64() == Some(-32601) {
        "codex_cli_incompatible"
    } else if text.contains("429") || text.contains("rate limit") {
        "rate_limited"
    } else if text.contains("401")
        || text.contains("unauthorized")
        || text.contains("not logged")
        || text.contains("not authenticated")
    {
        "local_login_expired"
    } else if text.contains("403") || text.contains("forbidden") {
        "permission_denied"
    } else {
        "cli_query_failed"
    };
    FetchError::new(code)
}
async fn query_with_binary(
    path: &Path,
    network: &NetworkConfig,
    timeout: Duration,
) -> Result<Vec<QuotaMetric>, FetchError> {
    let mut command = tokio::process::Command::new(path);
    command
        .args([
            "app-server",
            "--listen",
            "stdio://",
            "-c",
            "analytics.enabled=false",
            "-c",
            "mcp_servers={}",
            "-c",
            "chatgpt_base_url=\"https://chatgpt.com/backend-api\"",
        ])
        .current_dir(
            path.parent()
                .ok_or_else(|| FetchError::new("codex_cli_not_found"))?,
        );
    query_with_command(command, network, timeout).await
}
async fn query_with_command(
    mut command: tokio::process::Command,
    network: &NetworkConfig,
    timeout: Duration,
) -> Result<Vec<QuotaMetric>, FetchError> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    configure_network(&mut command, network)?;
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| FetchError::new("cli_launch_failed"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| FetchError::new("cli_launch_failed"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| FetchError::new("cli_launch_failed"))?;
    let mut lines = BufReader::new(stdout.take(262144)).lines();
    let result=tokio::time::timeout(timeout,async {
        rpc(&mut stdin,&mut lines,1,"initialize",Some(json!({"clientInfo":{"name":"stillenough","title":"StillEnough","version":"0.1.0"},"capabilities":{"experimentalApi":false}}))).await?;
        stdin.write_all(b"{\"method\":\"initialized\"}\n").await.map_err(|_|FetchError::new("cli_query_failed"))?;
        let account=rpc(&mut stdin,&mut lines,2,"account/read",Some(json!({"refreshToken":false}))).await?;
        if account["account"].is_null() { return Err(FetchError::new("local_login_required")); }
        if account["account"]["type"].as_str()!=Some("chatgpt") { return Err(FetchError::new("codex_subscription_required")); }
        let limits=rpc(&mut stdin,&mut lines,3,"account/rateLimits/read",None).await?;
        parse(&limits,&chrono::Utc::now().to_rfc3339())
    }).await.unwrap_or_else(|_|Err(FetchError::new("cli_timeout")));
    drop(stdin);
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}
async fn query(network: &NetworkConfig) -> Result<Vec<QuotaMetric>, FetchError> {
    tokio::time::timeout(Duration::from_secs(40), async {
        let path = discover().await?;
        query_with_binary(&path, network, Duration::from_secs(30)).await
    })
    .await
    .unwrap_or_else(|_| Err(FetchError::new("cli_timeout")))
}
fn parse(value: &Value, now: &str) -> Result<Vec<QuotaMetric>, FetchError> {
    let mut metrics = Vec::new();
    let buckets = if let Some(map) = value["rateLimitsByLimitId"]
        .as_object()
        .filter(|m| !m.is_empty())
    {
        map.iter().map(|(k, v)| (k.as_str(), v)).collect::<Vec<_>>()
    } else {
        vec![("codex", &value["rateLimits"])]
    };
    for (bucket, limits) in buckets {
        if bucket.len() > 80
            || !bucket
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ['_', '-', '.'].contains(&c))
        {
            return Err(FetchError::new("schema_changed"));
        }
        for field in ["primary", "secondary"] {
            let w = &limits[field];
            if w.is_null() {
                continue;
            }
            let duration = match w.get("windowDurationMins").filter(|v| !v.is_null()) {
                Some(v) => Some(
                    v.as_u64()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| FetchError::new("schema_changed"))?,
                ),
                None => None,
            };
            let window = match duration {
                Some(300) => "5 小时窗口".into(),
                Some(10080) => "周额度".into(),
                Some(n) => format!("{n} 分钟窗口"),
                None => {
                    if field == "primary" {
                        "主窗口".into()
                    } else {
                        "次窗口".into()
                    }
                }
            };
            metrics.push(super::subscription::window(
                format!("codex_{bucket}_{field}"),
                if bucket == "codex" {
                    window
                } else {
                    format!("{bucket} · {window}")
                },
                &w["usedPercent"],
                super::subscription::unix_reset(&w["resetsAt"])?,
                now,
            )?);
        }
    }
    if metrics.is_empty() {
        return Err(FetchError::new("schema_changed"));
    }
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    fn mock_command(account: &str, limits: &str) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(
            PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        let script=r#"
            $ErrorActionPreference='Stop'
            while ($line=[Console]::ReadLine()) {
                $request=$line | ConvertFrom-Json
                switch ($request.method) {
                    'initialize' { [Console]::WriteLine('{"method":"account/updated","params":{}}'); [Console]::WriteLine('{"id":1,"result":{}}') }
                    'initialized' { }
                    'account/read' {
                        if ($request.params.refreshToken -ne $false) { exit 8 }
                        [Console]::WriteLine('ACCOUNT_RESPONSE')
                    }
                    'account/rateLimits/read' { [Console]::WriteLine('LIMIT_RESPONSE') }
                    default { exit 9 }
                }
            }
        "#.replace("ACCOUNT_RESPONSE",account).replace("LIMIT_RESPONSE",limits);
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        command
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn rpc_process_handshake_login_and_errors_are_read_only() {
        let network = NetworkConfig {
            mode: "direct".into(),
            proxy_url: String::new(),
        };
        let account = r#"{"id":2,"result":{"account":{"type":"chatgpt","email":"synthetic@example.invalid"}}}"#;
        let limits=json!({"id":3,"result":serde_json::from_slice::<Value>(include_bytes!("../../../fixtures/providers/codex-rate-limits.json")).unwrap()}).to_string();
        assert_eq!(
            query_with_command(
                mock_command(account, &limits),
                &network,
                Duration::from_secs(10)
            )
            .await
            .unwrap()
            .len(),
            3
        );
        assert_eq!(
            query_with_command(
                mock_command(r#"{"id":2,"result":{"account":null}}"#, &limits),
                &network,
                Duration::from_secs(10)
            )
            .await
            .unwrap_err()
            .code,
            "local_login_required"
        );
        assert_eq!(
            query_with_command(
                mock_command(
                    r#"{"id":2,"result":{"account":{"type":"apiKey"}}}"#,
                    &limits
                ),
                &network,
                Duration::from_secs(10)
            )
            .await
            .unwrap_err()
            .code,
            "codex_subscription_required"
        );
        for (error, code) in [
            (
                json!({"code":-32601,"message":"synthetic secret"}),
                "codex_cli_incompatible",
            ),
            (
                json!({"code":-32000,"message":"401 unauthorized sensitive"}),
                "local_login_expired",
            ),
            (
                json!({"code":-32000,"message":"429 rate limit sensitive"}),
                "rate_limited",
            ),
        ] {
            let limits = json!({"id":3,"error":error}).to_string();
            assert_eq!(
                query_with_command(
                    mock_command(account, &limits),
                    &network,
                    Duration::from_secs(10)
                )
                .await
                .unwrap_err()
                .code,
                code
            );
        }
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn child_timeout_and_invalid_output_do_not_hang() {
        let network = NetworkConfig {
            mode: "direct".into(),
            proxy_url: String::new(),
        };
        let powershell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = tokio::process::Command::new(&powershell);
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 5",
        ]);
        assert_eq!(
            query_with_command(command, &network, Duration::from_millis(100))
                .await
                .unwrap_err()
                .code,
            "cli_timeout"
        );
        let mut command = tokio::process::Command::new(&powershell);
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::WriteLine(('x' * 270000))",
        ]);
        assert_eq!(
            query_with_command(command, &network, Duration::from_secs(10))
                .await
                .unwrap_err()
                .code,
            "schema_changed"
        );
    }
    #[cfg(windows)]
    #[test]
    fn discovers_npm_native_binary_and_rejects_workspace_binary() {
        let directory = std::env::temp_dir().join(format!(
            "stillenough-codex-discovery-{}",
            uuid::Uuid::new_v4()
        ));
        let (platform, triple) = if cfg!(target_arch = "aarch64") {
            ("codex-win32-arm64", "aarch64-pc-windows-msvc")
        } else {
            ("codex-win32-x64", "x86_64-pc-windows-msvc")
        };
        let package = directory.join("node_modules/@openai/codex");
        let binary = package.join(format!(
            "node_modules/@openai/{platform}/vendor/{triple}/bin/codex.exe"
        ));
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(package.join("package.json"), br#"{"name":"@openai/codex"}"#).unwrap();
        std::fs::write(&binary, b"synthetic-not-executed").unwrap();
        assert_eq!(discover_in(std::slice::from_ref(&directory)).len(), 1);
        std::fs::write(package.join("package.json"), br#"{"name":"unrelated"}"#).unwrap();
        assert!(discover_in(std::slice::from_ref(&directory)).is_empty());
        assert!(
            super::super::bailian_cli::trusted(Path::new(env!("CARGO_MANIFEST_DIR"))).is_none()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn multi_bucket_fixture_does_not_double_count_legacy_view() {
        let value: Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/providers/codex-rate-limits.json"
        ))
        .unwrap();
        let metrics = parse(&value, "now").unwrap();
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].remaining_percent, Some(75.0));
        assert_eq!(metrics[1].remaining_percent, Some(0.0));
        assert_eq!(metrics[2].remaining_percent, Some(100.0));
        assert!(parse(
            &json!({"rateLimits":{"primary":{"usedPercent":101}}}),
            "now"
        )
        .is_err());
        assert!(parse(&json!({"rateLimits":{"primary":null}}), "now").is_err());
        let legacy = parse(&json!({"rateLimits":{"primary":{"usedPercent":0}}}), "now").unwrap();
        assert_eq!(legacy[0].remaining_percent, Some(100.0));
        assert_eq!(legacy[0].resets_at, None);
    }
    #[test]
    fn routing_is_explicit_and_never_mutates_parent_environment() {
        let mut command = tokio::process::Command::new("unused");
        configure_network(
            &mut command,
            &NetworkConfig {
                mode: "direct".into(),
                proxy_url: String::new(),
            },
        )
        .unwrap();
        assert!(command
            .as_std()
            .get_envs()
            .any(|(key, value)| key == "HTTPS_PROXY" && value.is_none()));
        let mut command = tokio::process::Command::new("unused");
        configure_network(
            &mut command,
            &NetworkConfig {
                mode: "custom".into(),
                proxy_url: "http://127.0.0.1:7890".into(),
            },
        )
        .unwrap();
        assert!(command
            .as_std()
            .get_envs()
            .any(|(key, value)| key == "HTTPS_PROXY"
                && value == Some(std::ffi::OsStr::new("http://127.0.0.1:7890"))));
        assert!(configure_network(
            &mut command,
            &NetworkConfig {
                mode: "custom".into(),
                proxy_url: "socks5h://127.0.0.1:1080".into()
            }
        )
        .is_err());
    }
    #[tokio::test]
    #[ignore = "read-only installed Codex app-server query; emits no raw account response"]
    async fn query_installed_codex() {
        let result = query(&NetworkConfig::system()).await;
        match result {
            Ok(metrics) => {
                assert!(!metrics.is_empty());
                println!("Codex normalized metrics: {}", metrics.len());
            }
            Err(error) => panic!("Codex query status: {}", error.code),
        }
    }
}
