//! Codex HTTP subscription query, gated by explicit local-login authorization.
use super::{FetchError, FetchFuture, Provider};
use crate::domain::models::{ProviderDescriptor, QuotaMetric};
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use zeroize::{Zeroize, Zeroizing};

pub struct CodexHttpProvider;
pub const SOURCE: &str = "local://codex/subscription-http";
const ENDPOINT: &str = "https://chatgpt.com/backend-api/wham/usage";
impl Provider for CodexHttpProvider {
    fn requires_credential(&self) -> bool {
        false
    }
    fn descriptor(&self) -> ProviderDescriptor {
        stillenough_core::providers::descriptor("codex_http").expect("registered provider")
    }
    fn validate_url(&self, url: &str) -> Result<(), String> {
        if url == SOURCE {
            Ok(())
        } else {
            Err("请选择 Codex 本地登录 HTTP 来源".into())
        }
    }
    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
        url: &'a str,
        _: &'a str,
    ) -> FetchFuture<'a> {
        Box::pin(async move {
            self.validate_url(url)
                .map_err(|_| FetchError::new("unsupported"))?;
            let credentials = tauri::async_runtime::spawn_blocking(read_credentials)
                .await
                .map_err(|_| FetchError::new("local_credentials_unavailable"))??;
            read_usage(client, ENDPOINT, &credentials).await
        })
    }
}
#[derive(Deserialize)]
struct Login {
    auth_mode: Option<String>,
    tokens: Option<Tokens>,
}
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    account_id: Option<String>,
}
impl Drop for Tokens {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(id) = &mut self.account_id {
            id.zeroize();
        }
    }
}
struct Credentials {
    token: Zeroizing<String>,
    account: Zeroizing<String>,
}
fn valid_header(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|b| b.is_ascii_graphic())
}
fn parse_credentials(bytes: &[u8], now: i64) -> Result<Credentials, FetchError> {
    // Typed deserialization skips refresh_token, id_token and unrelated secrets.
    let login: Login = serde_json::from_slice(bytes)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    if login
        .auth_mode
        .as_deref()
        .is_some_and(|mode| mode != "chatgpt")
    {
        return Err(FetchError::new("codex_subscription_required"));
    }
    let mut tokens = login
        .tokens
        .ok_or_else(|| FetchError::new("codex_subscription_required"))?;
    if !valid_header(&tokens.access_token, 16384) {
        return Err(FetchError::new("local_credentials_unavailable"));
    }
    let account = tokens
        .account_id
        .as_deref()
        .filter(|id| valid_header(id, 256))
        .ok_or_else(|| FetchError::new("codex_account_unavailable"))?;
    // Decode expiry only. Unverified claims do not establish identity or permission.
    let parts: Vec<_> = tokens.access_token.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(FetchError::new("local_credentials_unavailable"));
    }
    let payload = Zeroizing::new(
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| FetchError::new("local_credentials_unavailable"))?,
    );
    #[derive(Deserialize)]
    struct Expiry {
        exp: Option<i64>,
    }
    let expiry: Expiry = serde_json::from_slice(&payload)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    if expiry.exp.is_some_and(|exp| exp <= now) {
        return Err(FetchError::new("local_login_expired"));
    }
    Ok(Credentials {
        account: Zeroizing::new(account.to_owned()),
        token: Zeroizing::new(std::mem::take(&mut tokens.access_token)),
    })
}
fn path_for(home: Option<PathBuf>, user: Option<PathBuf>) -> Result<PathBuf, FetchError> {
    let home = home
        .or_else(|| user.map(|p| p.join(".codex")))
        .ok_or_else(|| FetchError::new("local_credentials_unavailable"))?;
    if !home.is_absolute() {
        return Err(FetchError::new("local_credentials_unavailable"));
    }
    Ok(home.join("auth.json"))
}
fn read_credentials_at(path: &Path, now: i64) -> Result<Credentials, FetchError> {
    let file =
        std::fs::File::open(path).map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    if bytes.len() > 65536 {
        return Err(FetchError::new("local_credentials_unavailable"));
    }
    parse_credentials(&bytes, now)
}
fn read_credentials() -> Result<Credentials, FetchError> {
    let home = std::env::var_os("CODEX_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    read_credentials_at(&path_for(home, user)?, chrono::Utc::now().timestamp())
}
async fn read_usage(
    client: &reqwest::Client,
    endpoint: &str,
    credentials: &Credentials,
) -> Result<Vec<QuotaMetric>, FetchError> {
    let bearer = Zeroizing::new(format!("Bearer {}", &*credentials.token));
    let mut authorization = reqwest::header::HeaderValue::from_str(&bearer)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    authorization.set_sensitive(true);
    let mut account = reqwest::header::HeaderValue::from_str(&credentials.account)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    account.set_sensitive(true);
    let mut response = client
        .get(endpoint)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .header("ChatGPT-Account-Id", account)
        .header("Accept", "application/json")
        .header("User-Agent", "StillEnough/0.1.0")
        .send()
        .await
        .map_err(|e| {
            FetchError::new(if e.is_timeout() {
                "network_timeout"
            } else {
                "network_error"
            })
        })?;
    if let Some(mut error) = super::deepseek::status_error(
        response.status().as_u16(),
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
    ) {
        if error.code == "auth_required" {
            error.code = "local_login_expired";
        }
        return Err(error);
    }
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        FetchError::new(if e.is_timeout() {
            "network_timeout"
        } else {
            "network_error"
        })
    })? {
        if body.len() + chunk.len() > 65536 {
            return Err(FetchError::new("schema_changed"));
        }
        body.extend_from_slice(&chunk);
    }
    parse(
        &body,
        &chrono::Utc::now().to_rfc3339(),
        &credentials.account,
    )
}
// An exhausted ordinary pool is still a valid observation, needed to display Reserve.
fn exhausted(limit: &Value) -> bool {
    ["primary_window", "secondary_window"]
        .iter()
        .any(|field| limit[*field]["used_percent"].as_f64() == Some(100.0))
}
fn blocked(limit: &Value) -> Result<(), FetchError> {
    if limit.is_null() {
        return Ok(());
    }
    if !limit.is_object() {
        return Err(FetchError::new("schema_changed"));
    }
    for field in ["allowed", "limit_reached"] {
        if !limit[field].is_null() && !limit[field].is_boolean() {
            return Err(FetchError::new("schema_changed"));
        }
    }
    if (limit["allowed"] == false || limit["limit_reached"] == true) && !exhausted(limit) {
        return Err(FetchError::new("codex_usage_blocked"));
    }
    Ok(())
}
fn windows(
    limit: &Value,
    id: &str,
    title: &str,
    now: &str,
    metrics: &mut Vec<QuotaMetric>,
) -> Result<(), FetchError> {
    blocked(limit)?;
    for field in ["primary_window", "secondary_window"] {
        let w = &limit[field];
        if w.is_null() {
            continue;
        }
        let seconds = w["limit_window_seconds"]
            .as_i64()
            .filter(|s| *s > 0 && *s <= 31536000)
            .ok_or_else(|| FetchError::new("schema_changed"))?;
        let label = match seconds {
            18000 => "5 小时窗口".into(),
            604800 => "周额度".into(),
            n => format!("{} 分钟窗口", n as f64 / 60.0),
        };
        metrics.push(super::subscription::window(
            format!("codex_http_{id}_{field}"),
            format!("{title}{label}"),
            &w["used_percent"],
            super::subscription::unix_reset(&w["reset_at"])?,
            now,
        )?);
    }
    Ok(())
}
/// Parse only caller-supplied response bytes; this function never loads a login or queries a server.
pub fn parse(
    body: &[u8],
    now: &str,
    expected_account: &str,
) -> Result<Vec<QuotaMetric>, FetchError> {
    if body.len() > 65536 {
        return Err(FetchError::new("schema_changed"));
    }
    let value: Value =
        serde_json::from_slice(body).map_err(|_| FetchError::new("schema_changed"))?;
    if !value.is_object() {
        return Err(FetchError::new("schema_changed"));
    }
    if let Some(returned) = value.get("account_id").filter(|v| !v.is_null()) {
        if returned.as_str() != Some(expected_account) {
            return Err(FetchError::new("codex_account_mismatch"));
        }
    }
    let ordinary_exhaustion = value["rate_limit_reached_type"]["kind"] == "rate_limit_reached"
        && exhausted(&value["rate_limit"]);
    let other_restriction = !value["rate_limit_reached_type"].is_null() && !ordinary_exhaustion;
    if other_restriction {
        return Err(FetchError::new("codex_usage_blocked"));
    }
    let spend = &value["spend_control"];
    if !spend.is_null() {
        if !spend.is_object() || (!spend["reached"].is_null() && !spend["reached"].is_boolean()) {
            return Err(FetchError::new("schema_changed"));
        }
        if spend["reached"] == true {
            return Err(FetchError::new("codex_usage_blocked"));
        }
    }
    let mut metrics = Vec::new();
    windows(&value["rate_limit"], "codex", "", now, &mut metrics)?;
    if !value["additional_rate_limits"].is_null() {
        let entries = value["additional_rate_limits"]
            .as_array()
            .filter(|a| a.len() <= 32)
            .ok_or_else(|| FetchError::new("schema_changed"))?;
        let mut seen = std::collections::HashSet::new();
        for entry in entries {
            let id = entry["metered_feature"]
                .as_str()
                .filter(|id| {
                    !id.is_empty()
                        && id.len() <= 80
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                })
                .ok_or_else(|| FetchError::new("schema_changed"))?;
            if id == "codex" || !seen.insert(id) {
                return Err(FetchError::new("schema_changed"));
            }
            windows(
                &entry["rate_limit"],
                id,
                &format!("{id} · "),
                now,
                &mut metrics,
            )?;
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
    fn synthetic_login(exp: i64) -> Vec<u8> {
        let token = format!(
            "e30.{}.synthetic",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"))
        );
        serde_json::to_vec(&serde_json::json!({"auth_mode":"chatgpt","tokens":{"access_token":token,"account_id":"synthetic-account","refresh_token":{"ignored":"even its type"},"id_token":null}})).unwrap()
    }
    #[test]
    fn token_expiry_headers_and_storage_paths_are_checked() {
        let bytes = synthetic_login(2000);
        assert!(parse_credentials(&bytes, 1000).is_ok());
        assert_eq!(
            parse_credentials(&bytes, 2000).err().unwrap().code,
            "local_login_expired"
        );
        assert_eq!(
            parse_credentials(br#"{"auth_mode":"apikey"}"#, 0)
                .err()
                .unwrap()
                .code,
            "codex_subscription_required"
        );
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["tokens"]["account_id"] = "injected\r\nheader".into();
        assert_eq!(
            parse_credentials(&serde_json::to_vec(&value).unwrap(), 0)
                .err()
                .unwrap()
                .code,
            "codex_account_unavailable"
        );
        assert!(path_for(Some(PathBuf::from("relative")), None).is_err());
        let directory =
            std::env::temp_dir().join(format!("stillenough-codex-http-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let file = path_for(Some(directory.clone()), None).unwrap();
        std::fs::write(&file, &bytes).unwrap();
        assert!(read_credentials_at(&file, 1000).is_ok());
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        std::fs::write(&file, vec![b'x'; 65537]).unwrap();
        assert!(read_credentials_at(&file, 1000).is_err());
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[tokio::test]
    async fn request_headers_status_limits_and_timeout_are_safe() {
        use std::io::{Read, Write};
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_millis(300))
            .build()
            .unwrap();
        for (status, body, expected) in [
            (
                200,
                include_str!("../../../fixtures/providers/codex-http-usage.json").to_owned(),
                "ok",
            ),
            (401, "secret".into(), "local_login_expired"),
            (403, "secret".into(), "permission_denied"),
            (429, "secret".into(), "rate_limited"),
            (302, "secret".into(), "redirect_rejected"),
            (200, "<html>challenge</html>".into(), "schema_changed"),
            (200, "x".repeat(65537), "schema_changed"),
            (200, "delay".into(), "network_timeout"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let task = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let n = socket.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..n]).to_ascii_lowercase();
                assert!(request.contains("authorization: bearer synthetic-only"));
                assert!(request.contains("chatgpt-account-id: synthetic-account"));
                assert!(!request.contains("refresh_token"));
                if body == "delay" {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                }
                let _ = socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nRetry-After: 120\r\nLocation: http://127.0.0.1:1/forbidden\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes());
            });
            let credentials = Credentials {
                token: Zeroizing::new("synthetic-only".into()),
                account: Zeroizing::new("synthetic-account".into()),
            };
            match read_usage(&client, &format!("http://{address}/usage"), &credentials).await {
                Ok(metrics) => {
                    assert_eq!(expected, "ok");
                    assert_eq!(metrics.len(), 3);
                }
                Err(e) => {
                    assert_eq!(e.code, expected);
                    if status == 429 {
                        assert_eq!(e.retry_after_seconds, 120);
                    }
                }
            }
            task.join().unwrap();
        }
    }
    #[tokio::test]
    #[ignore = "reads authorized local Codex login and queries fixed usage endpoint only via 127.0.0.1:1090; run explicitly"]
    async fn query_authorized_local_login_via_user_proxy() {
        let config = crate::services::network::NetworkConfig {
            mode: "custom".into(),
            proxy_url: "http://127.0.0.1:1090".into(),
        };
        let client = crate::services::network::build(&config).unwrap();
        let credentials = read_credentials().unwrap_or_else(|e| panic!("local login: {}", e.code));
        let metrics = read_usage(&client, ENDPOINT, &credentials)
            .await
            .unwrap_or_else(|e| panic!("usage query: {}", e.code));
        println!(
            "Codex HTTP query succeeded via explicit user proxy; {} windows",
            metrics.len()
        );
        for metric in metrics {
            println!(
                "{}: remaining={}%, resets={}",
                metric.label,
                metric.remaining_percent.unwrap(),
                metric.resets_at.as_deref().unwrap_or("unknown")
            );
        }
    }
    fn fixture() -> Value {
        serde_json::from_slice(include_bytes!(
            "../../../fixtures/providers/codex-http-usage.json"
        ))
        .unwrap()
    }
    fn parsed(value: &Value) -> Result<Vec<QuotaMetric>, FetchError> {
        parse(
            &serde_json::to_vec(value).unwrap(),
            "2026-10-10T00:00:00Z",
            "synthetic-account",
        )
    }
    #[test]
    fn windows_use_reported_duration_and_preserve_zero_and_unknown_reset() {
        let metrics = parsed(&fixture()).unwrap();
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].remaining_percent, Some(75.0));
        assert_eq!(metrics[1].remaining_percent, Some(0.0));
        assert_eq!(metrics[2].remaining_percent, Some(100.0));
        assert_eq!(metrics[2].resets_at, None);
        assert!(metrics[0].label.contains("5 小时"));
        assert_eq!(metrics[0].source, "client_endpoint");
        assert!(metrics
            .iter()
            .all(|m| m.remaining.is_none() && m.limit.is_none()));
        let mut value = fixture();
        value["rate_limit"]["primary_window"]["limit_window_seconds"] = 3600.into();
        assert_eq!(parsed(&value).unwrap()[0].label, "60 分钟窗口");
    }
    #[test]
    fn server_restrictions_override_apparent_remaining_quota() {
        for field in ["allowed", "limit_reached"] {
            let mut value = fixture();
            value["rate_limit"][field] = (field == "limit_reached").into();
            value["rate_limit"]["secondary_window"]["used_percent"] = 20.into();
            assert_eq!(parsed(&value).unwrap_err().code, "codex_usage_blocked");
        }
        let mut value = fixture();
        value["spend_control"] = serde_json::json!({"reached":true});
        assert_eq!(parsed(&value).unwrap_err().code, "codex_usage_blocked");
        value = fixture();
        value["rate_limit_reached_type"] =
            serde_json::json!({"kind":"workspace_member_usage_limit_reached"});
        assert_eq!(parsed(&value).unwrap_err().code, "codex_usage_blocked");
    }
    #[test]
    fn exhausted_main_pool_remains_observable_for_reserve_display() {
        let mut value = fixture();
        value["rate_limit"]["allowed"] = false.into();
        value["rate_limit"]["limit_reached"] = true.into();
        value["rate_limit_reached_type"] = serde_json::json!({"kind":"rate_limit_reached"});
        let metrics = parsed(&value).unwrap();
        assert_eq!(metrics[1].remaining_percent, Some(0.0));
        assert_eq!(metrics[2].remaining_percent, Some(100.0));
        value["spend_control"] = serde_json::json!({"reached":true});
        assert_eq!(parsed(&value).unwrap_err().code, "codex_usage_blocked");
    }
    #[test]
    fn missing_invalid_or_duplicate_data_never_generates_fake_quota() {
        for bytes in [br#"{}"#.as_slice(),br#"[]"#,br#"{"rate_limit":null}"#,br#"{"rate_limit":{"allowed":"true"}}"#,
            br#"{"rate_limit":{"primary_window":{"used_percent":101,"limit_window_seconds":18000}}}"#,
            br#"{"rate_limit":{"primary_window":{"used_percent":20,"limit_window_seconds":0}}}"#,
            br#"<html>challenge</html>"#] {
            assert_eq!(parse(bytes,"now","a").unwrap_err().code,"schema_changed");
        }
        let mut value = fixture();
        let duplicate = value["additional_rate_limits"][0].clone();
        value["additional_rate_limits"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert_eq!(parsed(&value).unwrap_err().code, "schema_changed");
        assert_eq!(
            parse(&vec![b'x'; 65537], "now", "a").unwrap_err().code,
            "schema_changed"
        );
    }
    #[test]
    fn identity_mismatch_is_rejected_and_absence_is_not_verification() {
        let mut value = fixture();
        value["account_id"] = "different-synthetic-account".into();
        assert_eq!(parsed(&value).unwrap_err().code, "codex_account_mismatch");
        value.as_object_mut().unwrap().remove("account_id");
        assert!(parsed(&value).is_ok());
    }
}
