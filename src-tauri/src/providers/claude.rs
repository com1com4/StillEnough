use super::{FetchError, FetchFuture, Provider};
use crate::domain::models::{ProviderDescriptor, QuotaMetric};
use serde::Deserialize;
use serde_json::Value;
use std::{io::Read, path::PathBuf};
use zeroize::{Zeroize, Zeroizing};

pub struct ClaudeProvider;
pub const SOURCE: &str = "local://claude-code/subscription";
const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
impl Provider for ClaudeProvider {
    fn requires_credential(&self) -> bool {
        false
    }
    fn descriptor(&self) -> ProviderDescriptor {
        stillenough_core::providers::descriptor("claude_code").expect("registered provider")
    }
    fn validate_url(&self, url: &str) -> Result<(), String> {
        if url == SOURCE {
            Ok(())
        } else {
            Err("请选择 Claude Code 本地订阅来源".into())
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
            let token = tauri::async_runtime::spawn_blocking(read_token)
                .await
                .map_err(|_| FetchError::new("local_credentials_unavailable"))??;
            read_usage(client, ENDPOINT, &token).await
        })
    }
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<OAuth>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuth {
    access_token: String,
    expires_at: Option<i64>,
    scopes: Option<Vec<String>>,
}
impl Drop for OAuth {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}
fn parse_token(bytes: &[u8], now_ms: i64) -> Result<Zeroizing<String>, FetchError> {
    // Unknown fields, including refreshToken, are skipped and never retained in a value tree.
    let parsed: Credentials = serde_json::from_slice(bytes)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    let mut oauth = parsed
        .oauth
        .ok_or_else(|| FetchError::new("claude_subscription_required"))?;
    if oauth.expires_at.is_some_and(|at| at <= now_ms) {
        return Err(FetchError::new("local_login_expired"));
    }
    if oauth.access_token.is_empty()
        || oauth.access_token.len() > 16384
        || oauth
            .access_token
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(FetchError::new("local_credentials_unavailable"));
    }
    if oauth
        .scopes
        .as_ref()
        .is_some_and(|s| !s.iter().any(|s| s == "user:profile"))
    {
        return Err(FetchError::new("permission_denied"));
    }
    Ok(Zeroizing::new(std::mem::take(&mut oauth.access_token)))
}
fn credential_path() -> Result<PathBuf, FetchError> {
    let directory =
        if let Some(path) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|p| !p.is_empty()) {
            PathBuf::from(path)
        } else {
            PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .ok_or_else(|| FetchError::new("local_credentials_unavailable"))?,
            )
            .join(".claude")
        };
    if !directory.is_absolute() {
        return Err(FetchError::new("local_credentials_unavailable"));
    }
    Ok(directory.join(".credentials.json"))
}
fn read_token() -> Result<Zeroizing<String>, FetchError> {
    let file = std::fs::File::open(credential_path()?)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| FetchError::new("local_credentials_unavailable"))?;
    if bytes.len() > 65536 {
        return Err(FetchError::new("local_credentials_unavailable"));
    }
    parse_token(&bytes, chrono::Utc::now().timestamp_millis())
}
async fn read_usage(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
) -> Result<Vec<QuotaMetric>, FetchError> {
    let mut response = client
        .get(endpoint)
        .bearer_auth(token)
        .header("anthropic-beta", "oauth-2025-04-20")
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
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| FetchError::new("network_error"))?
    {
        if body.len() + chunk.len() > 65536 {
            return Err(FetchError::new("schema_changed"));
        }
        body.extend_from_slice(&chunk);
    }
    parse(&body, &chrono::Utc::now().to_rfc3339())
}
fn parse(body: &[u8], now: &str) -> Result<Vec<QuotaMetric>, FetchError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| FetchError::new("schema_changed"))?;
    if !value.is_object() {
        return Err(FetchError::new("schema_changed"));
    }
    let mut metrics = Vec::new();
    for (field, label) in [
        ("five_hour", "5 小时窗口"),
        ("seven_day", "周额度"),
        ("seven_day_sonnet", "Sonnet 周额度"),
        ("seven_day_opus", "Opus 周额度"),
        ("seven_day_oauth_apps", "OAuth 应用周额度"),
    ] {
        let w = &value[field];
        if w.is_null() {
            continue;
        }
        metrics.push(super::subscription::window(
            format!("claude_{field}"),
            label.into(),
            &w["utilization"],
            super::subscription::iso_reset(&w["resets_at"])?,
            now,
        )?);
    }
    if metrics.is_empty() {
        return Err(FetchError::new("schema_changed"));
    }
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_preserves_windows_zero_and_missing_values() {
        let metrics = parse(
            include_bytes!("../../../fixtures/providers/claude-usage.json"),
            "now",
        )
        .unwrap();
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].remaining_percent, Some(75.0));
        assert_eq!(metrics[1].remaining_percent, Some(0.0));
        assert_eq!(metrics[2].remaining_percent, Some(100.0));
        assert!(parse(br#"{"five_hour":null}"#, "now").is_err());
        assert!(parse(br#"{"five_hour":{"utilization":101}}"#, "now").is_err());
        assert!(parse(
            br#"{"five_hour":{"utilization":20,"resets_at":"invalid"}}"#,
            "now"
        )
        .is_err());
    }
    #[test]
    fn token_is_read_only_expiry_and_scope_are_checked() {
        let bytes = br#"{"claudeAiOauth":{"accessToken":"synthetic-only","refreshToken":"ignored","expiresAt":2000,"scopes":["user:profile"]}}"#;
        assert_eq!(&**parse_token(bytes, 1000).unwrap(), "synthetic-only");
        assert_eq!(
            parse_token(bytes, 2000).unwrap_err().code,
            "local_login_expired"
        );
        assert!(parse_token(br#"{"apiKey":"synthetic"}"#, 0).is_err());
        assert_eq!(
            parse_token(
                br#"{"claudeAiOauth":{"accessToken":"synthetic","scopes":["user:inference"]}}"#,
                0
            )
            .unwrap_err()
            .code,
            "permission_denied"
        );
    }
    #[tokio::test]
    async fn http_auth_rate_limit_and_redirect_errors_never_expose_payload() {
        use std::io::{Read, Write};
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        for (status, code) in [
            (401, "local_login_expired"),
            (403, "permission_denied"),
            (429, "rate_limited"),
            (302, "redirect_rejected"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let task = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let n = socket.read(&mut request).unwrap();
                let text = String::from_utf8_lossy(&request[..n]);
                assert!(text.contains("Bearer synthetic-only"));
                assert!(text.contains("oauth-2025-04-20"));
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: 9\r\nRetry-After: 120\r\nConnection: close\r\n\r\nsensitive").as_bytes()).unwrap();
            });
            assert_eq!(
                read_usage(&client, &format!("http://{address}"), "synthetic-only")
                    .await
                    .unwrap_err()
                    .code,
                code
            );
            task.join().unwrap();
        }
    }
}
