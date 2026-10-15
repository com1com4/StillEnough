use super::{
    deepseek::{decimal, status_error},
    FetchError, FetchFuture, Provider,
};
use crate::domain::models::{ProviderDescriptor, QuotaMetric};
use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;

pub struct PlanProvider(pub &'static str);
const MINI: &[&str] = &["https://www.minimaxi.com", "https://www.minimax.io"];
const WALLET: &[&str] = &["https://business.aliyuncs.com"];
const MINI_WALLET: &[&str] = &["https://api.minimax.cn", "https://api.minimax.io"];

impl Provider for PlanProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        let (name, urls, auth, capability, limitation) = match self.0 {
            "minimax_api" => ("MiniMax API", MINI_WALLET, "api_key", "API 钱包可用金额 / 现金余额", "使用 sk-api- 普通 API Key；查询源为官方 CLI 使用的账户接口，可能变化。响应未给币种时明确显示币种未知，不推定 CNY 或 USD。"),
            "minimax_token_plan" => ("MiniMax Token Plan", MINI, "api_key", "Token Plan 通用额度 · 5 小时 / 周窗口", "中国/国际区域分别使用对应套餐 Key；只显示 general 额度，不把计数为零解释为耗尽或不限量。"),
            _ => ("阿里百炼 API（阿里云账户余额）", WALLET, "access_key_pair", "阿里云账户现金余额 / 可用额度", "需要 AccessKey ID / Secret 及 bss:DescribeAcccount 只读权限；这是整个阿里云账户余额，不是百炼专属余额或 Token Plan 额度。"),
        };
        ProviderDescriptor {
            id: self.0,
            name,
            supported: true,
            auth_method: auth,
            base_urls: urls.to_vec(),
            capability,
            status: "实验性 / 待真实账号验证",
            setup_instructions: "",
            authorization_label: "",
            network_note: "",
            limitation,
        }
    }
    fn validate_url(&self, url: &str) -> Result<(), String> {
        if self.descriptor().base_urls.contains(&url) {
            Ok(())
        } else {
            Err("请选择此产品支持的官方查询地址".into())
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
            let request = match self.0 {
                "minimax_api" => {
                    if !key.starts_with("sk-api-") {
                        return Err(FetchError::new("auth_required"));
                    }
                    client
                        .get(format!("{url}/account/query_balance"))
                        .bearer_auth(key)
                }
                "minimax_token_plan" => client
                    .get(format!("{url}/v1/token_plan/remains"))
                    .bearer_auth(key)
                    .header("Content-Type", "application/json"),
                _ => {
                    let credentials: Value =
                        serde_json::from_str(key).map_err(|_| FetchError::new("auth_required"))?;
                    let id = credentials["accessKeyId"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| FetchError::new("auth_required"))?;
                    let secret = credentials["accessKeySecret"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| FetchError::new("auth_required"))?;
                    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                    let nonce = uuid::Uuid::new_v4().to_string();
                    let mut params = BTreeMap::from([
                        ("AccessKeyId", id.to_string()),
                        ("Action", "QueryAccountBalance".into()),
                        ("Format", "JSON".into()),
                        ("Version", "2017-12-14".into()),
                        ("SignatureMethod", "HMAC-SHA1".into()),
                        ("SignatureVersion", "1.0".into()),
                        ("SignatureNonce", nonce),
                        ("Timestamp", timestamp),
                    ]);
                    let canonical = params
                        .iter()
                        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
                        .collect::<Vec<_>>()
                        .join("&");
                    let mut mac =
                        Hmac::<sha1::Sha1>::new_from_slice(format!("{secret}&").as_bytes())
                            .map_err(|_| FetchError::new("auth_required"))?;
                    mac.update(format!("GET&%2F&{}", encode(&canonical)).as_bytes());
                    params.insert("Signature", STANDARD.encode(mac.finalize().into_bytes()));
                    client.get(format!("{url}/")).query(&params)
                }
            };
            let body = read_response(request).await?;
            parse(self.0, &body, &chrono::Utc::now().to_rfc3339())
        })
    }
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

async fn read_response(request: reqwest::RequestBuilder) -> Result<Vec<u8>, FetchError> {
    let mut response = request
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
    Ok(body)
}
fn timestamp(v: &Value) -> Result<Option<String>, FetchError> {
    if v.is_null() {
        return Ok(None);
    }
    let millis = v
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| FetchError::new("schema_changed"))?;
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|d| Some(d.to_rfc3339()))
        .ok_or_else(|| FetchError::new("schema_changed"))
}
fn quota(
    id: &str,
    label: &str,
    percent: &Value,
    reset: &Value,
    used_ratio: bool,
    now: &str,
) -> Result<QuotaMetric, FetchError> {
    let remaining_percent = if percent.is_null() {
        None
    } else {
        let value = percent
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0 && *n <= if used_ratio { 1.0 } else { 100.0 })
            .ok_or_else(|| FetchError::new("schema_changed"))?;
        Some(if used_ratio {
            (1.0 - value) * 100.0
        } else {
            value
        })
    };
    Ok(QuotaMetric {
        metric_id: id.into(),
        label: label.into(),
        kind: "quota".into(),
        unit: "percent".into(),
        remaining: remaining_percent.map(|p| format!("{p:.1}%")),
        limit: None,
        remaining_percent,
        resets_at: timestamp(reset)?,
        source: "official_api".into(),
        observed_at: now.into(),
        stale_after_seconds: 1200,
    })
}
pub fn parse(id: &str, body: &[u8], now: &str) -> Result<Vec<QuotaMetric>, FetchError> {
    let root: Value =
        serde_json::from_slice(body).map_err(|_| FetchError::new("schema_changed"))?;
    let invalid = || FetchError::new("schema_changed");
    let mut metrics = match id {
        "minimax_api" => {
            match root["base_resp"]["status_code"].as_i64() {
                Some(0) => {}
                Some(1004 | 2049) => return Err(FetchError::new("auth_required")),
                Some(1002) => return Err(FetchError::new("rate_limited")),
                Some(_) => return Err(FetchError::new("unsupported")),
                None => return Err(invalid()),
            }
            let mut metrics = Vec::new();
            for (field, label) in [
                ("available_amount", "可用金额（币种未提供）"),
                ("cash_balance", "现金余额（币种未提供）"),
            ] {
                let value = root[field]
                    .as_str()
                    .filter(|s| decimal(s))
                    .ok_or_else(invalid)?;
                metrics.push(QuotaMetric {
                    metric_id: field.into(),
                    label: label.into(),
                    kind: "balance".into(),
                    unit: "currency_unknown".into(),
                    remaining: Some(value.into()),
                    limit: None,
                    remaining_percent: None,
                    resets_at: None,
                    source: "client_endpoint".into(),
                    observed_at: now.into(),
                    stale_after_seconds: 1200,
                });
            }
            metrics
        }
        "minimax_token_plan" => {
            match root["base_resp"]["status_code"].as_i64() {
                Some(0) => {}
                Some(1004 | 2049) => return Err(FetchError::new("auth_required")),
                Some(1002) => return Err(FetchError::new("rate_limited")),
                Some(_) => return Err(FetchError::new("unsupported")),
                None => return Err(invalid()),
            }
            let rows = root["model_remains"].as_array().ok_or_else(invalid)?;
            let rows: Vec<_> = rows
                .iter()
                .filter(|r| r["model_name"] == "general")
                .collect();
            if rows.len() != 1 {
                return Err(invalid());
            }
            let row = rows[0];
            let mut metrics = Vec::new();
            for (prefix, name, reset) in [
                ("current_interval", "5 小时额度", "end_time"),
                ("current_weekly", "周额度", "weekly_end_time"),
            ] {
                // Only explicitly active buckets carry a trustworthy percentage.
                let active = matches!(row[format!("{prefix}_status")].as_i64(), Some(1 | 2));
                let value = if active {
                    &row[format!("{prefix}_remaining_percent")]
                } else {
                    &Value::Null
                };
                metrics.push(quota(
                    prefix,
                    name,
                    value,
                    if active { &row[reset] } else { &Value::Null },
                    false,
                    now,
                )?);
            }
            metrics
        }
        "bailian_token_plan" => {
            if root["data"]["success"] == false {
                let code = root["data"]["errorCode"].as_str().unwrap_or("");
                return Err(FetchError::new(if code.contains("NotLogined") {
                    "auth_required"
                } else {
                    "permission_denied"
                }));
            }
            if root["data"]["success"] != true {
                return Err(invalid());
            }
            let usage = root
                .pointer("/data/DataV2/data/data")
                .or_else(|| root.pointer("/data/data"))
                .ok_or_else(invalid)?;
            let mut metrics = vec![
                quota(
                    "five_hour",
                    "5 小时额度",
                    &usage["per5HourPercentage"],
                    &usage["per5HourResetTime"],
                    true,
                    now,
                )?,
                quota(
                    "weekly",
                    "周额度",
                    &usage["per1WeekPercentage"],
                    &usage["per1WeekResetTime"],
                    true,
                    now,
                )?,
            ];
            if usage.get("per1MonthPercentage").is_some()
                || usage.get("per1MonthResetTime").is_some()
            {
                if [
                    "per5HourPercentage",
                    "per5HourResetTime",
                    "per1WeekPercentage",
                    "per1WeekResetTime",
                ]
                .iter()
                .all(|field| usage.get(field).is_none())
                {
                    metrics.clear();
                }
                metrics.push(quota(
                    "monthly",
                    "月额度",
                    &usage["per1MonthPercentage"],
                    &usage["per1MonthResetTime"],
                    true,
                    now,
                )?);
            }
            metrics
        }
        "bailian_api" => {
            if root["Success"] != true || root["Code"] != "200" {
                return Err(FetchError::new("permission_denied"));
            }
            let data = &root["Data"];
            let currency = data["Currency"]
                .as_str()
                .and_then(crate::domain::currency::normalize)
                .ok_or_else(invalid)?;
            let mut metrics = Vec::new();
            for (field, label) in [
                ("AvailableCashAmount", "阿里云账户现金余额"),
                ("AvailableAmount", "阿里云账户可用额度（含信用）"),
            ] {
                let value = data[field]
                    .as_str()
                    .filter(|s| decimal(s))
                    .ok_or_else(invalid)?;
                metrics.push(QuotaMetric {
                    metric_id: field.into(),
                    label: label.into(),
                    kind: "balance".into(),
                    unit: currency.clone(),
                    remaining: Some(format!("{value} {currency}")),
                    limit: None,
                    remaining_percent: None,
                    resets_at: None,
                    source: "official_api".into(),
                    observed_at: now.into(),
                    stale_after_seconds: 1200,
                });
            }
            metrics
        }
        _ => return Err(FetchError::new("unsupported")),
    };
    if !metrics.iter().any(|m| m.remaining.is_some()) {
        return Err(invalid());
    }
    if id == "bailian_token_plan" {
        for metric in &mut metrics {
            metric.source = "client_endpoint".into();
        }
    }
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn http_errors_and_response_limits_do_not_expose_payload() {
        use std::io::{Read, Write};
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_millis(150))
            .build()
            .unwrap();
        for (status, body, delay, code) in [
            (401, "sensitive".to_owned(), 0, "auth_required"),
            (403, "sensitive".into(), 0, "permission_denied"),
            (429, "sensitive".into(), 0, "rate_limited"),
            (302, "sensitive".into(), 0, "redirect_rejected"),
            (500, "sensitive".into(), 0, "network_error"),
            (200, "x".repeat(65537), 0, "schema_changed"),
            (200, "{}".into(), 300, "network_timeout"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let n = socket.read(&mut request).unwrap();
                assert!(String::from_utf8_lossy(&request[..n])
                    .to_lowercase()
                    .contains("authorization: bearer synthetic-test-key"));
                std::thread::sleep(std::time::Duration::from_millis(delay));
                let _ = socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\nLocation: http://127.0.0.1:1\r\nRetry-After: 120\r\n\r\n{body}", body.len()).as_bytes());
            });
            let error = read_response(
                client
                    .get(format!("http://{address}"))
                    .bearer_auth("synthetic-test-key"),
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, code);
            if status == 429 {
                assert_eq!(error.retry_after_seconds, 120);
            }
        }
    }
    #[test]
    fn wallet_currency_uses_shared_validation() {
        let mut body: Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/providers/bailian-balance.json"
        ))
        .unwrap();
        for code in [" eur ", "HKD", "JPY"] {
            body["Data"]["Currency"] = json!(code);
            let result = parse("bailian_api", body.to_string().as_bytes(), "now").unwrap();
            assert_eq!(result[0].unit, code.trim().to_ascii_uppercase());
            assert_eq!(
                result[0].remaining.as_deref(),
                Some(format!("0.00 {}", result[0].unit).as_str())
            );
        }
        for code in [json!("ZZZ"), json!(""), json!(null), json!(123)] {
            body["Data"]["Currency"] = code;
            assert_eq!(
                parse("bailian_api", body.to_string().as_bytes(), "now")
                    .unwrap_err()
                    .code,
                "schema_changed"
            );
        }
    }
    #[test]
    fn fixtures_preserve_direction_wallet_and_missing_windows() {
        let mini_wallet = parse(
            "minimax_api",
            include_bytes!("../../../fixtures/providers/minimax-balance.json"),
            "now",
        )
        .unwrap();
        assert_eq!(mini_wallet[0].remaining.as_deref(), Some("12.50"));
        assert_eq!(mini_wallet[0].unit, "currency_unknown");
        assert_eq!(mini_wallet[0].source, "client_endpoint");
        assert!(mini_wallet.iter().all(|m| m.remaining_percent.is_none()));
        let mini = parse(
            "minimax_token_plan",
            include_bytes!("../../../fixtures/providers/minimax-token-plan.json"),
            "now",
        )
        .unwrap();
        assert_eq!(mini[0].remaining_percent, Some(63.0));
        assert_eq!(mini[1].remaining_percent, None);
        let ali = parse(
            "bailian_token_plan",
            include_bytes!("../../../fixtures/providers/bailian-token-plan.json"),
            "now",
        )
        .unwrap();
        assert_eq!(ali[0].remaining_percent, Some(75.0));
        assert_eq!(ali[1].remaining_percent, Some(0.0));
        let wallet = parse(
            "bailian_api",
            include_bytes!("../../../fixtures/providers/bailian-balance.json"),
            "now",
        )
        .unwrap();
        assert_eq!(wallet[0].remaining.as_deref(), Some("0.00 CNY"));
        assert!(wallet.iter().all(|m| m.remaining_percent.is_none()));
    }
    #[test]
    fn rejects_login_errors_unknown_and_invalid_percentages() {
        assert_eq!(
            parse(
                "bailian_token_plan",
                br#"{"data":{"success":false,"errorCode":"BailianGateway.Login.NotLogined"}}"#,
                "now"
            )
            .unwrap_err()
            .code,
            "auth_required"
        );
        for body in [
            br#"{"data":{"success":true,"data":{"per5HourPercentage":2}}}"#.as_slice(),
            br#"{"data":{"success":true,"data":{}}}"#,
        ] {
            assert!(parse("bailian_token_plan", body, "now").is_err());
        }
        for id in ["minimax_api", "minimax_token_plan", "bailian_api"] {
            assert!(parse(id, b"{}", "now").is_err());
            let provider = PlanProvider(id);
            assert!(provider
                .validate_url(&format!("{}.evil.test", provider.descriptor().base_urls[0]))
                .is_err());
            assert!(provider.validate_url("http://localhost").is_err());
        }
        assert_eq!(encode("a b/+~"), "a%20b%2F%2B~");
    }
    #[test]
    fn exhausted_minimax_and_invalid_reset_are_distinct() {
        let mut response: Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/providers/minimax-token-plan.json"
        ))
        .unwrap();
        response["model_remains"][0]["current_interval_status"] = json!(2);
        response["model_remains"][0]["current_interval_remaining_percent"] = json!(0);
        let result = parse(
            "minimax_token_plan",
            &serde_json::to_vec(&response).unwrap(),
            "now",
        )
        .unwrap();
        assert_eq!(result[0].remaining_percent, Some(0.0));
        response["model_remains"][0]["end_time"] = json!("bad-date");
        assert!(parse(
            "minimax_token_plan",
            &serde_json::to_vec(&response).unwrap(),
            "now"
        )
        .is_err());
        assert!(parse(
            "minimax_api",
            br#"{"base_resp":{"status_code":1004},"available_amount":"0"}"#,
            "now"
        )
        .is_err());
    }
}
