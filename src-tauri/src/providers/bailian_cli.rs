use super::{FetchError, FetchFuture, Provider};
use crate::domain::models::{ProviderDescriptor, QuotaMetric};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncReadExt;

pub struct BailianCliProvider;
pub const SOURCE: &str = "cli://bailian/token-plan";
const ARGS: &[&str] = &[
    "usage",
    "token-plan",
    "--output",
    "json",
    "--console-site",
    "domestic",
    "--console-region",
    "cn-beijing",
];
// Same read-only endpoint as usage token-plan. CLI 2.1.0 drops monthly fields
// from its convenience command; console call preserves the gateway response.
const RAW_ARGS: &[&str] = &[
    "console",
    "call",
    "--api",
    "zeldaHttp.apikeyMgr./tokenplan/personal/api/v2/usage",
    "--data",
    "{}",
    "--output",
    "json",
    "--console-site",
    "domestic",
    "--console-region",
    "cn-beijing",
];
impl Provider for BailianCliProvider {
    fn network_kind(&self) -> &'static str {
        "cli_managed"
    }
    fn requires_credential(&self) -> bool {
        false
    }
    fn descriptor(&self) -> ProviderDescriptor {
        stillenough_core::providers::descriptor("bailian_token_plan").expect("registered provider")
    }
    fn validate_url(&self, url: &str) -> Result<(), String> {
        if url == SOURCE {
            Ok(())
        } else {
            Err("此产品仅支持已授权的官方百炼 CLI 来源".into())
        }
    }
    fn fetch<'a>(&'a self, _: &'a reqwest::Client, url: &'a str, _: &'a str) -> FetchFuture<'a> {
        Box::pin(async move {
            self.validate_url(url)
                .map_err(|_| FetchError::new("unsupported"))?;
            let candidates = discover()?;
            let launch = select(candidates).await?;
            query(&launch).await
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Launch {
    program: PathBuf,
    prefix: Vec<std::ffi::OsString>,
    directory: PathBuf,
}

// The running application's own directories are excluded independently of launcher cwd.
pub(super) fn trusted(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let resolved = path.canonicalize().ok()?;
    #[cfg(debug_assertions)]
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
    #[cfg(debug_assertions)]
    if let Ok(root) = workspace.canonicalize() {
        if resolved.starts_with(root) {
            return None;
        }
    }
    if let Ok(exe) = std::env::current_exe().and_then(|path| path.canonicalize()) {
        if exe.parent().is_some_and(|root| resolved.starts_with(root)) {
            return None;
        }
    }
    Some(resolved)
}

fn expand_path(value: &str) -> String {
    let mut result = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest[1..].find('%').map(|i| i + 1) else {
            break;
        };
        let name = &rest[1..end];
        match std::env::var(name) {
            Ok(value) => result.push_str(&value),
            Err(_) => result.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}

pub(super) fn search_paths() -> Vec<PathBuf> {
    let mut values = vec![std::env::var_os("PATH").unwrap_or_default()];
    #[cfg(windows)]
    {
        use winreg::{
            enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ},
            RegKey,
        };
        // Read freshly registered paths without modifying the app or system environment.
        for (hive, key) in [
            (HKEY_CURRENT_USER, "Environment"),
            (
                HKEY_LOCAL_MACHINE,
                r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            ),
        ] {
            if let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(key, KEY_READ) {
                if let Ok(value) = key.get_value::<String, _>("Path") {
                    values.push(expand_path(&value).into());
                }
            }
        }
    }
    let mut paths = Vec::new();
    for value in values {
        for path in std::env::split_paths(&value).filter_map(|p| trusted(&p)) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

fn npm_entry(root: &Path) -> Option<PathBuf> {
    let manifest = std::fs::File::open(root.join("package.json")).ok()?;
    // Bound reads even for broken/unrelated packages.
    let package: serde_json::Value =
        serde_json::from_reader(std::io::Read::take(manifest, 65536)).ok()?;
    if package.get("name")?.as_str()? != "bailian-cli" {
        return None;
    }
    let bin = package.get("bin")?;
    let entry = bin
        .get("bl")
        .or_else(|| bin.get("bailian"))
        .unwrap_or(bin)
        .as_str()?;
    let relative = Path::new(entry);
    if relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
            )
        })
    {
        return None;
    }
    let resolved = trusted(&root.join(relative))?;
    let root = root.canonicalize().ok()?;
    (resolved.starts_with(root) && resolved.is_file()).then_some(resolved)
}

fn discover_in(paths: &[PathBuf]) -> Result<Vec<Launch>, FetchError> {
    let mut launches = Vec::new();
    let mut error = "cli_not_found";
    for directory in paths {
        for name in ["bl.exe", "bailian.exe"] {
            if let Some(program) = trusted(&directory.join(name)).filter(|p| p.is_file()) {
                let launch = Launch {
                    program,
                    prefix: vec![],
                    directory: directory.clone(),
                };
                if !launches.contains(&launch) {
                    launches.push(launch);
                }
            }
        }
        if ["bl.cmd", "bailian.cmd", "bl.ps1", "bailian.ps1"]
            .iter()
            .any(|name| directory.join(name).is_file())
        {
            error = "cli_install_invalid";
            let Some(script) = npm_entry(&directory.join("node_modules/bailian-cli")) else {
                continue;
            };
            error = "cli_node_missing";
            // Match npm shims: adjacent node first, then the remaining PATH runtimes.
            for runtime in std::iter::once(directory).chain(paths.iter()) {
                if let Some(program) = trusted(&runtime.join("node.exe")).filter(|p| p.is_file()) {
                    let launch = Launch {
                        program,
                        prefix: vec![script.clone().into_os_string()],
                        directory: directory.clone(),
                    };
                    if !launches.contains(&launch) {
                        launches.push(launch);
                    }
                }
            }
        }
    }
    if launches.is_empty() {
        Err(FetchError::new(error))
    } else {
        Ok(launches)
    }
}
fn discover() -> Result<Vec<Launch>, FetchError> {
    discover_in(&search_paths())
}

fn version_tuple(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}
fn cli_version(body: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(body) else {
        return false;
    };
    ["bl ", "bailian ", "bailian-cli "].iter().any(|prefix| {
        text.trim()
            .strip_prefix(prefix)
            .and_then(version_tuple)
            .is_some()
    })
}
fn token_plan_help(body: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(body) else {
        return false;
    };
    text.contains("usage token-plan")
        && ["--output", "--console-site", "--console-region"]
            .iter()
            .all(|flag| text.contains(flag))
}
async fn probe(launch: &Launch) -> Result<(), FetchError> {
    if !launch.prefix.is_empty() {
        let node = Launch {
            program: launch.program.clone(),
            prefix: vec![],
            directory: launch.directory.clone(),
        };
        let body = probe_output(&node, &["--version"], Duration::from_secs(3)).await?;
        if std::str::from_utf8(&body)
            .ok()
            .and_then(version_tuple)
            .is_none_or(|v| v < (18, 17, 0))
        {
            return Err(FetchError::new("cli_node_incompatible"));
        }
    }
    let body = probe_output(launch, &["--version"], Duration::from_secs(3)).await?;
    if !cli_version(&body) {
        return Err(FetchError::new("cli_identity_mismatch"));
    }
    let body = probe_output(
        launch,
        &["usage", "token-plan", "--help"],
        Duration::from_secs(3),
    )
    .await?;
    if !token_plan_help(&body) {
        return Err(FetchError::new("cli_version_unsupported"));
    }
    Ok(())
}
async fn select(candidates: Vec<Launch>) -> Result<Launch, FetchError> {
    select_with(candidates, |launch| async move { probe(&launch).await }).await
}
async fn select_with<F, Fut>(candidates: Vec<Launch>, check: F) -> Result<Launch, FetchError>
where
    F: Fn(Launch) -> Fut,
    Fut: std::future::Future<Output = Result<(), FetchError>>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut error = FetchError::new("cli_not_found");
    for launch in candidates {
        match tokio::time::timeout_at(deadline, check(launch.clone())).await {
            Ok(Ok(())) => return Ok(launch),
            Ok(Err(e)) => error = e,
            Err(_) => return Err(FetchError::new("cli_detection_timeout")),
        }
    }
    Err(error)
}
async fn query(launch: &Launch) -> Result<Vec<QuotaMetric>, FetchError> {
    let body = execute(launch, ARGS, Duration::from_secs(30)).await?;
    let now = chrono::Utc::now().to_rfc3339();
    match parse(&body, &now) {
        Err(error) if error.code == "cli_quota_unavailable" => {
            let body = execute(launch, RAW_ARGS, Duration::from_secs(30)).await?;
            parse_gateway(&body, &now)
        }
        result => result,
    }
}
fn parse_gateway(body: &[u8], now: &str) -> Result<Vec<QuotaMetric>, FetchError> {
    let root: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| FetchError::new("schema_changed"))?;
    if root.get("successResponse") == Some(&serde_json::Value::Bool(false)) {
        return Err(FetchError::new("cli_query_failed"));
    }
    if root["data"]["success"] != true {
        return super::plans::parse("bailian_token_plan", body, now).map_err(|error| {
            if error.code == "auth_required" {
                FetchError::new("cli_auth_required")
            } else {
                error
            }
        });
    }
    if let Some(inner) = root.pointer("/data/DataV2/data") {
        if inner["success"] != true {
            return Err(FetchError::new("cli_query_failed"));
        }
    }
    let usage = root
        .pointer("/data/DataV2/data/data")
        .or_else(|| root.pointer("/data/data"))
        .ok_or_else(|| FetchError::new("schema_changed"))?;
    parse(
        &serde_json::to_vec(usage).map_err(|_| FetchError::new("schema_changed"))?,
        now,
    )
}
#[cfg(test)]
async fn run(launch: Launch, timeout: Duration) -> Result<Vec<u8>, FetchError> {
    execute(&launch, ARGS, timeout).await
}
async fn execute(launch: &Launch, args: &[&str], timeout: Duration) -> Result<Vec<u8>, FetchError> {
    execute_capture(launch, args, timeout, false).await
}
async fn probe_output(
    launch: &Launch,
    args: &[&str],
    timeout: Duration,
) -> Result<Vec<u8>, FetchError> {
    // CLI help may intentionally be written to stderr. Only fixed version/help probes
    // capture it; quota queries still discard stderr and never expose raw errors.
    execute_capture(launch, args, timeout, true)
        .await
        .map_err(|error| match error.code {
            "cli_timeout" => FetchError::new("cli_detection_timeout"),
            "cli_query_failed" | "schema_changed" => FetchError::new("cli_version_unsupported"),
            _ => error,
        })
}
async fn execute_capture(
    launch: &Launch,
    args: &[&str],
    timeout: Duration,
    capture_stderr: bool,
) -> Result<Vec<u8>, FetchError> {
    let mut command = tokio::process::Command::new(&launch.program);
    command.args(&launch.prefix);
    command
        .args(args)
        .current_dir(&launch.directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = command
        .spawn()
        .map_err(|_| FetchError::new("cli_launch_failed"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| FetchError::new("cli_query_failed"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| FetchError::new("cli_query_failed"))?;
    let result = tokio::time::timeout(timeout, async {
        let read = async {
            let mut body = Vec::new();
            stdout
                .take(65537)
                .read_to_end(&mut body)
                .await
                .map_err(|_| FetchError::new("cli_query_failed"))?;
            if body.len() > 65536 {
                return Err(FetchError::new("schema_changed"));
            }
            Ok(body)
        };
        let drain = async {
            if !capture_stderr {
                let size = tokio::io::copy(&mut stderr.take(65537), &mut tokio::io::sink())
                    .await
                    .map_err(|_| FetchError::new("cli_query_failed"))?;
                if size > 65536 {
                    return Err(FetchError::new("cli_query_failed"));
                }
                return Ok(Vec::new());
            }
            let mut body = Vec::new();
            stderr
                .take(65537)
                .read_to_end(&mut body)
                .await
                .map_err(|_| FetchError::new("cli_query_failed"))?;
            if body.len() > 65536 {
                return Err(FetchError::new("cli_query_failed"));
            }
            Ok(body)
        };
        let wait = async {
            child
                .wait()
                .await
                .map_err(|_| FetchError::new("cli_query_failed"))
        };
        let (mut body, stderr, status) = tokio::try_join!(read, drain, wait)?;
        if !status.success() {
            return Err(FetchError::new("cli_query_failed"));
        }
        if capture_stderr {
            body.extend_from_slice(&stderr);
            if body.len() > 65536 {
                return Err(FetchError::new("schema_changed"));
            }
        }
        Ok(body)
    })
    .await;
    match result {
        Ok(Ok(body)) => Ok(body),
        Ok(Err(error)) => {
            let _ = child.kill().await;
            Err(error)
        }
        Err(_) => {
            let _ = child.kill().await;
            Err(FetchError::new("cli_timeout"))
        }
    }
}
pub fn parse(body: &[u8], now: &str) -> Result<Vec<QuotaMetric>, FetchError> {
    let usage: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| FetchError::new("schema_changed"))?;
    // Accept only the command's documented flat result, not arbitrary gateway/error JSON.
    if !usage.is_object() || usage.get("error").is_some() {
        return Err(FetchError::new("schema_changed"));
    }
    // The official CLI can successfully return {} when no numeric quota is
    // available. This is distinct from malformed or changed response fields.
    let fields = [
        "per5HourPercentage",
        "per5HourResetTime",
        "per1WeekPercentage",
        "per1WeekResetTime",
        "per1MonthPercentage",
        "per1MonthResetTime",
    ];
    if usage
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
        && [
            "per5HourPercentage",
            "per1WeekPercentage",
            "per1MonthPercentage",
        ]
        .iter()
        .all(|key| usage.get(key).is_none_or(serde_json::Value::is_null))
    {
        return Err(FetchError::new("cli_quota_unavailable"));
    }
    let wrapped = serde_json::json!({"data":{"success":true,"data":usage}});
    super::plans::parse(
        "bailian_token_plan",
        &serde_json::to_vec(&wrapped).map_err(|_| FetchError::new("schema_changed"))?,
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn npm_manifest_resolves_declared_entry_and_rejects_escape() {
        let root = std::env::temp_dir().join(format!("apm-npm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("lib/cli.mjs"), "synthetic").unwrap();
        let manifest = root.join("package.json");
        for (body, valid) in [
            (
                r#"{"name":"bailian-cli","bin":{"bailian":"lib/cli.mjs"}}"#,
                true,
            ),
            (r#"{"name":"other","bin":{"bl":"lib/cli.mjs"}}"#, false),
            (
                r#"{"name":"bailian-cli","bin":{"bl":"../escape.mjs"}}"#,
                false,
            ),
            (
                r#"{"name":"bailian-cli","bin":{"bl":"missing.mjs"}}"#,
                false,
            ),
        ] {
            std::fs::write(&manifest, body).unwrap();
            assert_eq!(npm_entry(&root).is_some(), valid);
        }
        std::fs::remove_file(manifest).unwrap();
        std::fs::remove_file(root.join("lib/cli.mjs")).unwrap();
        std::fs::remove_dir(root.join("lib")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn multiple_installations_and_adjacent_node_are_discovered_without_cwd_dependency() {
        let root = std::env::temp_dir().join(format!("apm-paths-{}", uuid::Uuid::new_v4()));
        let first = root.join("runtime-a");
        let npm = root.join("npm");
        let package = npm.join("node_modules/bailian-cli");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(package.join("custom")).unwrap();
        for file in [
            first.join("node.exe"),
            npm.join("node.exe"),
            npm.join("bailian.ps1"),
            npm.join("bailian.exe"),
            package.join("custom/main.mjs"),
        ] {
            std::fs::write(file, "synthetic").unwrap();
        }
        std::fs::write(
            package.join("package.json"),
            r#"{"name":"bailian-cli","bin":{"bailian":"custom/main.mjs"}}"#,
        )
        .unwrap();
        let paths = [trusted(&first).unwrap(), trusted(&npm).unwrap()];
        let candidates = discover_in(&paths).unwrap();
        assert_eq!(candidates.len(), 3);
        assert_eq!(
            candidates[0].program,
            trusted(&npm.join("bailian.exe")).unwrap()
        );
        assert_eq!(
            candidates[1].program,
            trusted(&npm.join("node.exe")).unwrap()
        );
        assert_eq!(
            candidates[2].program,
            trusted(&first.join("node.exe")).unwrap()
        );
        assert_eq!(
            candidates[1].prefix[0],
            trusted(&package.join("custom/main.mjs")).unwrap()
        );
        std::fs::remove_file(npm.join("bailian.exe")).unwrap();
        std::fs::remove_file(npm.join("node.exe")).unwrap();
        std::fs::remove_file(first.join("node.exe")).unwrap();
        assert_eq!(discover_in(&paths).unwrap_err().code, "cli_node_missing");
        std::fs::remove_file(package.join("package.json")).unwrap();
        assert_eq!(discover_in(&paths).unwrap_err().code, "cli_install_invalid");
        std::fs::remove_file(npm.join("bailian.ps1")).unwrap();
        assert_eq!(discover_in(&paths).unwrap_err().code, "cli_not_found");
        std::fs::remove_file(package.join("custom/main.mjs")).unwrap();
        for dir in [
            package.join("custom"),
            package,
            npm.join("node_modules"),
            npm,
            first,
            root,
        ] {
            std::fs::remove_dir(dir).unwrap();
        }
    }
    #[tokio::test]
    async fn invalid_first_candidate_does_not_hide_valid_installation() {
        let candidate = |name| Launch {
            program: PathBuf::from(name),
            prefix: vec![],
            directory: PathBuf::from("synthetic"),
        };
        let selected = select_with(
            vec![candidate("wrong"), candidate("valid")],
            |launch| async move {
                if launch.program == Path::new("valid") {
                    Ok(())
                } else {
                    Err(FetchError::new("cli_identity_mismatch"))
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(selected.program, Path::new("valid"));
    }
    #[test]
    fn identity_and_capability_are_both_required() {
        assert!(cli_version(b"bl 2.1.0\n"));
        assert!(!cli_version(b"unrelated 2.1.0"));
        assert!(!cli_version(b"bl garbage"));
        assert!(version_tuple("v18.17.0").unwrap() >= (18, 17, 0));
        assert!(version_tuple("v18.16.9").unwrap() < (18, 17, 0));
        assert!(token_plan_help(
            b"usage token-plan --output --console-site --console-region"
        ));
        assert!(!token_plan_help(b"Usage: bl --help"));
        assert_eq!(
            expand_path("abc%APM_MISSING_VARIABLE_9843%tail"),
            "abc%APM_MISSING_VARIABLE_9843%tail"
        );
    }
    #[test]
    #[ignore = "read-only local discovery; no account queries"]
    fn discovers_installed_cli_without_running_it() {
        let candidates = discover().expect("installed official CLI should be discoverable");
        assert!(!candidates.is_empty());
        assert!(candidates.iter().all(|launch| launch.program.is_file()));
    }
    #[tokio::test]
    #[ignore = "local version/help probes only; no login or account queries"]
    async fn verifies_installed_cli_without_querying_account() {
        select(discover().unwrap()).await.unwrap();
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn child_success_failure_timeout_and_output_limit() {
        let directory = std::env::temp_dir().join(format!("apm-cli-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let program = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script = directory.join("fixture.ps1");
        for (content, expected) in [
            (
                "[Console]::Out.Write('{\"per5HourPercentage\":0.25}'); exit 0",
                None,
            ),
            (
                "[Console]::Error.Write('synthetic-sensitive-text'); exit 3",
                Some("cli_query_failed"),
            ),
            (
                "[Console]::Out.Write(('x' * 65537)); exit 0",
                Some("schema_changed"),
            ),
            ("Start-Sleep -Seconds 10", Some("cli_timeout")),
        ] {
            std::fs::write(&script, content).unwrap();
            let launch = Launch {
                program: program.clone(),
                prefix: vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-File".into(),
                    script.clone().into_os_string(),
                ],
                directory: directory.clone(),
            };
            let result = run(
                launch,
                if expected == Some("cli_timeout") {
                    Duration::from_millis(150)
                } else {
                    Duration::from_secs(10)
                },
            )
            .await;
            if let Some(code) = expected {
                assert_eq!(result.unwrap_err().code, code);
            } else {
                assert_eq!(
                    parse(&result.unwrap(), "now").unwrap()[0].remaining_percent,
                    Some(75.0)
                );
            }
        }
        std::fs::write(&script, "[Console]::Error.Write('usage token-plan --output --console-site --console-region'); exit 0").unwrap();
        let help = Launch {
            program,
            prefix: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-File".into(),
                script.clone().into_os_string(),
            ],
            directory: directory.clone(),
        };
        assert!(token_plan_help(
            &probe_output(&help, &["--help"], Duration::from_secs(10))
                .await
                .unwrap()
        ));
        assert!(execute(&help, ARGS, Duration::from_secs(10))
            .await
            .unwrap()
            .is_empty());
        std::fs::remove_file(script).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn gateway_monthly_quota_is_preserved_and_errors_are_not_success() {
        let body = br#"{"successResponse":true,"data":{"success":true,"DataV2":{"data":{"success":true,"data":{"per1MonthPercentage":0.25,"per1MonthResetTime":1791507600000}}}}}"#;
        let metrics = parse_gateway(body, "now").unwrap();
        assert_eq!(metrics.last().unwrap().metric_id, "monthly");
        assert_eq!(metrics.last().unwrap().remaining_percent, Some(75.0));
        assert_eq!(metrics.len(), 1);
        assert!(parse_gateway(br#"{"data":{"success":true,"DataV2":{"data":{"success":false,"data":{"per1MonthPercentage":0}}}}}"#, "now").is_err());
        assert_eq!(
            parse_gateway(
                br#"{"data":{"success":false,"errorCode":"BailianGateway.Login.NotLogined"}}"#,
                "now"
            )
            .unwrap_err()
            .code,
            "cli_auth_required"
        );
    }
    #[test]
    fn empty_official_result_is_unavailable_not_schema_changed() {
        for body in [
            b"{}".as_slice(),
            br#"{"per5HourPercentage":null}"#,
            br#"{"per5HourResetTime":1790902800000}"#,
        ] {
            assert_eq!(
                parse(body, "now").unwrap_err().code,
                "cli_quota_unavailable"
            );
        }
        for body in [
            br#"{"renamedQuota":25}"#.as_slice(),
            br#"{"error":"synthetic"}"#,
            br#"{"per5HourPercentage":2}"#,
            br#"{"per5HourPercentage":"0.25"}"#,
        ] {
            assert_eq!(parse(body, "now").unwrap_err().code, "schema_changed");
        }
        assert!(parse(br#"{"per5HourPercentage":0}"#, "now").is_ok());
    }
    #[tokio::test]
    #[ignore = "read-only real quota query; no raw output or credentials persisted"]
    async fn queries_current_cli_and_reports_only_normalized_status() {
        let launch = select(discover().unwrap()).await.unwrap();
        let metrics = query(&launch)
            .await
            .expect("current authorized CLI should report actual quota");
        assert!(metrics.iter().any(|metric| metric.remaining.is_some()));
        eprintln!(
            "normalized metric ids: {:?}",
            metrics
                .iter()
                .map(|metric| metric.metric_id.as_str())
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn flat_cli_output_preserves_used_direction_and_missing_window() {
        let result = parse(
            include_bytes!("../../../fixtures/providers/bailian-token-plan-cli.json"),
            "now",
        )
        .unwrap();
        assert_eq!(result[0].remaining_percent, Some(75.0));
        assert_eq!(result[1].remaining_percent, Some(0.0));
        assert_eq!(result[0].source, "client_endpoint");
        let result = parse(br#"{"per5HourPercentage":0}"#, "now").unwrap();
        assert_eq!(result[0].remaining_percent, Some(100.0));
        assert_eq!(result[1].remaining_percent, None);
        for body in [
            b"{}".as_slice(),
            b"not-json",
            br#"{"per5HourPercentage":2}"#,
            br#"{"error":"secret","per5HourPercentage":0}"#,
        ] {
            assert!(parse(body, "now").is_err());
        }
        #[cfg(debug_assertions)]
        assert!(trusted(Path::new(env!("CARGO_MANIFEST_DIR"))).is_none());
        assert!(trusted(Path::new(".")).is_none());
        assert!(!BailianCliProvider.requires_credential());
    }
}
