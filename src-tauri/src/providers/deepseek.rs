use super::{FetchError, FetchFuture, Provider};
use crate::domain::models::{ProviderDescriptor, QuotaMetric};
use serde::Deserialize;
use std::collections::HashSet;
pub struct DeepSeekProvider;
#[derive(Deserialize)]
struct BalanceResponse {
    is_available: bool,
    balance_infos: Vec<BalanceInfo>,
}
#[derive(Deserialize)]
struct BalanceInfo {
    currency: String,
    total_balance: String,
    granted_balance: String,
    topped_up_balance: String,
}
pub(super) fn decimal(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    !value.is_empty()
        && value.split('.').count() <= 2
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}
pub fn parse(body: &[u8], observed_at: &str) -> Result<Vec<QuotaMetric>, FetchError> {
    let response: BalanceResponse =
        serde_json::from_slice(body).map_err(|_| FetchError::new("schema_changed"))?;
    // is_available=false does not mean missing or a zero balance.
    let _available = response.is_available;
    if response.balance_infos.is_empty() {
        return Err(FetchError::new("schema_changed"));
    }
    let mut seen = HashSet::new();
    let mut metrics = Vec::new();
    for mut info in response.balance_infos {
        info.currency = crate::domain::currency::normalize(&info.currency)
            .ok_or_else(|| FetchError::new("schema_changed"))?;
        if !seen.insert(info.currency.clone())
            || ![
                &info.total_balance,
                &info.granted_balance,
                &info.topped_up_balance,
            ]
            .iter()
            .all(|value| decimal(value))
        {
            return Err(FetchError::new("schema_changed"));
        }
        metrics.push(QuotaMetric {
            metric_id: format!("balance_{}", info.currency),
            label: format!("可用余额 · {}", info.currency),
            kind: "balance".into(),
            unit: info.currency.clone(),
            remaining: Some(format!("{} {}", info.total_balance, info.currency)),
            limit: None,
            remaining_percent: None,
            resets_at: None,
            source: "official_api".into(),
            observed_at: observed_at.into(),
            stale_after_seconds: 1200,
        });
    }
    Ok(metrics)
}
pub fn status_error(status: u16, retry: Option<&str>) -> Option<FetchError> {
    let code = match status {
        200 => return None,
        401 => "auth_required",
        403 => "permission_denied",
        407 => "proxy_auth_required",
        429 => "rate_limited",
        300..=399 => "redirect_rejected",
        500..=599 => "network_error",
        _ => "unsupported",
    };
    let seconds = retry
        .and_then(|v| {
            v.parse::<u64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(v)
                    .ok()
                    .map(|date| (date.timestamp() - chrono::Utc::now().timestamp()).max(0) as u64)
            })
        })
        .unwrap_or(60)
        .max(30);
    if seconds > 604800 {
        return Some(FetchError::new("schema_changed"));
    }
    Some(FetchError {
        code,
        retry_after_seconds: seconds,
    })
}
impl Provider for DeepSeekProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "deepseek_api",
            name: "DeepSeek API",
            supported: true,
            auth_method: "api_key",
            base_urls: vec!["https://api.deepseek.com"],
            capability: "API 账号可用余额（按接口返回币种）",
            status: "实验性 / 待真实账号验证",
            setup_instructions: "",
            authorization_label: "",
            network_note: "",
            limitation:
                "只读取钱包余额，不读取编程套餐额度；接口不返回账号身份，按所配置的 Key 绑定",
        }
    }
    fn validate_url(&self, url: &str) -> Result<(), String> {
        // Exact approved origin, no paths, credentials, queries, fragments, or redirects.
        if url == "https://api.deepseek.com" {
            Ok(())
        } else {
            Err("API 地址仅支持 https://api.deepseek.com".into())
        }
    }
    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
        url: &'a str,
        key: &'a str,
    ) -> FetchFuture<'a> {
        Box::pin(async move {
            self.validate_url(url)
                .map_err(|_| FetchError::new("unsupported"))?;
            fetch_endpoint(client, &format!("{url}/user/balance"), key).await
        })
    }
}
async fn fetch_endpoint(
    client: &reqwest::Client,
    endpoint: &str,
    key: &str,
) -> Result<Vec<QuotaMetric>, FetchError> {
    let mut response = client
        .get(endpoint)
        .bearer_auth(key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| {
            FetchError::new(if e.is_timeout() {
                "network_timeout"
            } else {
                "network_error"
            })
        })?;
    if let Some(error) = status_error(
        response.status().as_u16(),
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
    ) {
        return Err(error);
    }
    let mut body = Vec::new();
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
    parse(&body, &chrono::Utc::now().to_rfc3339())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_wallet_without_percentage() {
        let result = parse(
            include_bytes!("../../../fixtures/providers/deepseek-balance.json"),
            "2026-10-01T00:00:00Z",
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].remaining.as_deref(), Some("12.50 CNY"));
        assert_eq!(result[0].remaining_percent, None);
    }
    #[test]
    fn normalizes_currency_and_rejects_duplicates_or_invalid_codes() {
        let body = |codes: &[&str]| {
            serde_json::json!({
                "is_available": true,
                "balance_infos": codes.iter().map(|code| serde_json::json!({
                    "currency": code, "total_balance": "12.50",
                    "granted_balance": "0", "topped_up_balance": "12.50"
                })).collect::<Vec<_>>()
            })
            .to_string()
        };
        let result = parse(body(&[" eur ", "HKD"]).as_bytes(), "now").unwrap();
        assert_eq!(result[0].unit, "EUR");
        assert_eq!(result[0].metric_id, "balance_EUR");
        assert_eq!(result[0].remaining.as_deref(), Some("12.50 EUR"));
        for codes in [&["usd", " USD "][..], &["ZZZ"], &[""]] {
            assert_eq!(
                parse(body(codes).as_bytes(), "now").unwrap_err().code,
                "schema_changed"
            );
        }
    }
    #[test]
    fn rejects_empty_invalid_or_ambiguous_response() {
        for body in [
            r#"{"is_available":true,"balance_infos":[]}"#,
            r#"{"is_available":true}"#,
            r#"{"is_available":false,"balance_infos":[{"currency":"USD","total_balance":"NaN","granted_balance":"0","topped_up_balance":"0"}]}"#,
        ] {
            assert!(parse(body.as_bytes(), "now").is_err());
        }
    }
    #[test]
    fn refuses_unapproved_urls() {
        for url in [
            "http://api.deepseek.com",
            "https://api.deepseek.com.evil.test",
            "https://user:secret@api.deepseek.com",
            "https://api.deepseek.com?key=x",
            "https://api.deepseek.com/v1",
            "https://api.deepseek.com#x",
        ] {
            assert!(DeepSeekProvider.validate_url(url).is_err());
        }
    }
    #[test]
    fn maps_errors_and_retry_after_without_response_text() {
        assert_eq!(status_error(401, None).unwrap().code, "auth_required");
        assert_eq!(status_error(403, None).unwrap().code, "permission_denied");
        assert_eq!(
            status_error(429, Some("120")).unwrap().retry_after_seconds,
            120
        );
        assert_eq!(status_error(302, None).unwrap().code, "redirect_rejected");
        assert!(status_error(200, None).is_none());
        assert_eq!(
            status_error(429, Some("18446744073709551615"))
                .unwrap()
                .code,
            "schema_changed"
        );
    }
    fn mock_server(status: u16, body: String, headers: &str, delay_ms: u64) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let headers = headers.to_owned();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).unwrap();
            assert!(String::from_utf8_lossy(&buffer[..n])
                .to_lowercase()
                .contains("authorization: bearer synthetic-test-key"));
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            let response=format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",body.len());
            let _ = socket.write_all(response.as_bytes());
        });
        format!("http://{address}/user/balance")
    }
    #[tokio::test]
    async fn http_success_errors_redirect_and_timeout() {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_millis(150))
            .build()
            .unwrap();
        let fixture = include_str!("../../../fixtures/providers/deepseek-balance.json");
        let url = mock_server(200, fixture.into(), "", 0);
        assert_eq!(
            fetch_endpoint(&client, &url, "synthetic-test-key")
                .await
                .unwrap()
                .len(),
            2
        );
        for (status, code) in [
            (401, "auth_required"),
            (403, "permission_denied"),
            (429, "rate_limited"),
            (302, "redirect_rejected"),
            (500, "network_error"),
        ] {
            let url = mock_server(
                status,
                "sensitive response text must not escape".into(),
                "Location: http://127.0.0.1:1/credential-leak\r\nRetry-After: 120\r\n",
                0,
            );
            assert_eq!(
                fetch_endpoint(&client, &url, "synthetic-test-key")
                    .await
                    .unwrap_err()
                    .code,
                code
            );
        }
        let url = mock_server(200, "{}".into(), "", 0);
        assert_eq!(
            fetch_endpoint(&client, &url, "synthetic-test-key")
                .await
                .unwrap_err()
                .code,
            "schema_changed"
        );
        let url = mock_server(200, fixture.into(), "", 400);
        assert_eq!(
            fetch_endpoint(&client, &url, "synthetic-test-key")
                .await
                .unwrap_err()
                .code,
            "network_timeout"
        );
    }
}
