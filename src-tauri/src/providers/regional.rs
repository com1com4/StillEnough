use super::{plans::read_response, FetchError, FetchFuture, Provider};
use crate::domain::models::{ProviderDescriptor, QuotaMetric};
use serde_json::Value;

pub struct RegionalProvider(pub &'static str);
impl Provider for RegionalProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        stillenough_core::providers::descriptor(self.0).expect("registered provider")
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
            self.validate_url(url).map_err(|_| invalid())?;
            let request = if self.0.starts_with("glm_") {
                client
                    .get(format!("{url}/api/monitor/usage/quota/limit"))
                    .header("Authorization", key)
            } else if self.0.starts_with("kimi_api_") {
                client
                    .get(format!("{url}/v1/users/me/balance"))
                    .bearer_auth(key)
            } else {
                client
                    .get(format!("{url}/coding/v1/usages"))
                    .bearer_auth(key)
            };
            let body = read_response(request).await?;
            parse(self.0, &body, &chrono::Utc::now().to_rfc3339())
        })
    }
}
fn invalid() -> FetchError {
    FetchError::new("schema_changed")
}
fn number(v: &Value) -> Result<f64, FetchError> {
    v.as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(invalid)
}
// The official Kimi CLI accepts counts encoded as strings as well as numbers.
// Keep this scoped to Kimi; other providers retain their own field contracts.
fn kimi_count(v: &Value) -> Result<f64, FetchError> {
    if let Some(text) = v.as_str() {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        return text
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(invalid);
    }
    number(v)
}
fn metric(
    id: String,
    label: String,
    percent: f64,
    reset: &Value,
    now: &str,
) -> Result<QuotaMetric, FetchError> {
    if !(0.0..=100.0).contains(&percent) {
        return Err(invalid());
    }
    let resets_at = if reset.is_null() {
        None
    } else if let Some(ms) = reset.as_i64() {
        Some(
            chrono::DateTime::from_timestamp_millis(ms)
                .filter(|_| ms > 0)
                .ok_or_else(invalid)?
                .to_rfc3339(),
        )
    } else {
        Some(
            chrono::DateTime::parse_from_rfc3339(reset.as_str().ok_or_else(invalid)?)
                .map_err(|_| invalid())?
                .to_rfc3339(),
        )
    };
    Ok(QuotaMetric {
        metric_id: id,
        label,
        kind: "quota".into(),
        unit: "percent".into(),
        remaining: Some(format!("{percent:.1}%")),
        limit: None,
        remaining_percent: Some(percent),
        resets_at,
        source: "client_endpoint".into(),
        observed_at: now.into(),
        stale_after_seconds: 1200,
    })
}
pub fn parse(id: &str, body: &[u8], now: &str) -> Result<Vec<QuotaMetric>, FetchError> {
    let root: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let mut result = Vec::new();
    if id.starts_with("kimi_api_") {
        if root["code"].as_i64() != Some(0)
            || root["status"] == false
            || root.get("error").is_some()
        {
            return Err(invalid());
        }
        let unit = if let Some(currency) = root["data"].get("currency") {
            crate::domain::currency::normalize(currency.as_str().ok_or_else(invalid)?)
                .ok_or_else(invalid)?
        } else if id == "kimi_api_intl" {
            "USD".into()
        } else {
            "currency_unknown".into()
        };
        for (field, label) in [
            ("available_balance", "可用余额"),
            ("voucher_balance", "代金券余额"),
            ("cash_balance", "现金余额"),
        ] {
            let Some(value) = root["data"].get(field) else {
                if field == "available_balance" {
                    return Err(invalid());
                }
                continue;
            };
            let amount = if let Some(text) = value.as_str() {
                text.to_owned()
            } else if value.is_number() {
                value.to_string()
            } else {
                return Err(invalid());
            };
            // Monetary values remain decimal text; negative arrears are valid.
            if !super::deepseek::decimal(&amount)
                || (field == "voucher_balance" && amount.starts_with('-'))
            {
                return Err(invalid());
            }
            result.push(QuotaMetric {
                metric_id: field.into(),
                label: label.into(),
                kind: "balance".into(),
                unit: unit.clone(),
                remaining: Some(if unit == "currency_unknown" {
                    amount
                } else {
                    format!("{amount} {unit}")
                }),
                limit: None,
                remaining_percent: None,
                resets_at: None,
                source: "official_api".into(),
                observed_at: now.into(),
                stale_after_seconds: 1200,
            });
        }
    } else if id.starts_with("glm_") {
        if root["success"] != true {
            return Err(FetchError::new(match root["code"].as_i64() {
                Some(401 | 1000 | 1001 | 1002) => "auth_required",
                Some(403) => "permission_denied",
                Some(429) => "rate_limited",
                _ => "schema_changed",
            }));
        }
        for row in root["data"]["limits"].as_array().ok_or_else(invalid)? {
            let (kind, label) = match row["type"].as_str() {
                Some("TOKENS_LIMIT") => ("tokens", "GLM Token 额度"),
                Some("CREDIT_LIMIT") => ("credits", "GLM Credit 额度"),
                Some("TIME_LIMIT") => ("tools", "GLM 工具调用额度"),
                _ => continue,
            };
            // Explicit units identify separate token/credit windows. Never infer
            // the period from reset order: a weekly window may reset first.
            let (key, label) = if kind != "tools" {
                match (row["unit"].as_u64(), row["number"].as_u64()) {
                    (Some(3), Some(hours)) if hours > 0 => (
                        format!("{kind}_hours_{hours}"),
                        format!("{} · {hours}小时额度", label.trim_end_matches(" 额度")),
                    ),
                    (Some(6), _) => (
                        format!("{kind}_weekly"),
                        format!("{} · 周额度", label.trim_end_matches(" 额度")),
                    ),
                    _ => (kind.to_owned(), label.to_owned()),
                }
            } else {
                (kind.to_owned(), label.to_owned())
            };
            // Ambiguous or duplicate identities still fail instead of overwriting.
            if result.iter().any(|m: &QuotaMetric| m.metric_id == key) {
                return Err(invalid());
            }
            if row["percentage"].is_null() {
                continue;
            }
            result.push(metric(
                key,
                label,
                100.0 - number(&row["percentage"])?,
                &row["nextResetTime"],
                now,
            )?);
        }
    } else {
        // Follow the official CLI's usage + limits/detail count schema.
        let mut rows = Vec::new();
        if root["usage"].is_object() {
            rows.push((
                "weekly".to_string(),
                "Kimi 周额度".to_string(),
                &root["usage"],
            ));
        }
        if let Some(limits) = root["limits"].as_array() {
            for (i, row) in limits.iter().enumerate() {
                let detail = row.get("detail").unwrap_or(row);
                let label = match (
                    row["window"]["duration"].as_u64(),
                    row["window"]["timeUnit"].as_str(),
                ) {
                    (Some(300), Some("TIME_UNIT_MINUTE" | "MINUTE"))
                    | (Some(5), Some("HOUR" | "TIME_UNIT_HOUR")) => "Kimi 5小时额度".to_string(),
                    _ => format!("Kimi 额度 {}", i + 1),
                };
                rows.push((format!("limit_{i}"), label, detail));
            }
        }
        for (key, label, row) in rows {
            if row["limit"].is_null() || (row["used"].is_null() && row["remaining"].is_null()) {
                continue;
            }
            let limit = kimi_count(&row["limit"])?;
            if limit == 0.0 {
                continue;
            }
            let remaining = if row["remaining"].is_null() {
                limit - kimi_count(&row["used"])?
            } else {
                kimi_count(&row["remaining"])?
            };
            if !row["used"].is_null()
                && !row["remaining"].is_null()
                && (kimi_count(&row["used"])? + remaining - limit).abs() > 0.000001
            {
                return Err(invalid());
            }
            let reset = ["resetTime", "reset_time", "resetAt", "reset_at"]
                .iter()
                .find_map(|k| row.get(k))
                .unwrap_or(&Value::Null);
            result.push(metric(key, label, remaining / limit * 100.0, reset, now)?);
        }
    }
    if result.is_empty() {
        return Err(invalid());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glm_explicit_windows_survive_reordering_without_guessing_periods() {
        let mut body: Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/providers/glm-coding-windows.json"
        ))
        .unwrap();
        for id in ["glm_coding_cn", "glm_coding_intl"] {
            let metrics = parse(id, body.to_string().as_bytes(), "now").unwrap();
            assert_eq!(metrics.len(), 3);
            assert_eq!(metrics[0].metric_id, "tokens_weekly");
            assert_eq!(metrics[0].remaining_percent, Some(25.0));
            assert_eq!(metrics[1].metric_id, "tokens_hours_5");
            assert_eq!(metrics[1].remaining_percent, Some(80.0));
            body["data"]["limits"].as_array_mut().unwrap().reverse();
            let reversed = parse(id, body.to_string().as_bytes(), "now").unwrap();
            for metric in &metrics {
                let other = reversed
                    .iter()
                    .find(|m| m.metric_id == metric.metric_id)
                    .unwrap();
                assert_eq!(other.remaining_percent, metric.remaining_percent);
                assert_eq!(other.resets_at, metric.resets_at);
                assert_eq!(other.label, metric.label);
            }
            body["data"]["limits"].as_array_mut().unwrap().reverse();
            let mut duplicate = body.clone();
            duplicate["data"]["limits"][1] = duplicate["data"]["limits"][0].clone();
            assert!(parse(id, duplicate.to_string().as_bytes(), "now").is_err());
            let mut ambiguous = body.clone();
            for row in ambiguous["data"]["limits"].as_array_mut().unwrap() {
                row.as_object_mut().unwrap().remove("unit");
            }
            assert!(parse(id, ambiguous.to_string().as_bytes(), "now").is_err());
            let mut missing = body.clone();
            missing["data"]["limits"][0]["percentage"] = Value::Null;
            let missing = parse(id, missing.to_string().as_bytes(), "now").unwrap();
            assert!(!missing.iter().any(|m| m.metric_id == "tokens_weekly"));
            let mut credit = body.clone();
            credit["data"]["limits"][0]["type"] = "CREDIT_LIMIT".into();
            let credit = parse(id, credit.to_string().as_bytes(), "now").unwrap();
            assert_eq!(credit[0].metric_id, "credits_weekly");
            assert_eq!(credit[0].kind, "quota");
        }
    }
    #[test]
    fn kimi_string_counts_preserve_remaining_and_validation() {
        for id in ["kimi_coding_cn", "kimi_coding_intl"] {
            let body = br#"{"usage":{"limit":"100","used":"20","remaining":"80"},"limits":[{"detail":{"limit":"50","remaining":"0"}}]}"#;
            let metrics = parse(id, body, "now").unwrap();
            assert_eq!(metrics[0].remaining_percent, Some(80.0));
            assert_eq!(metrics[1].remaining_percent, Some(0.0));
            assert!(metrics.iter().all(|m| m.resets_at.is_none()));
            for body in [
                r#"{"usage":{"limit":"100","used":"20","remaining":"90"}}"#,
                r#"{"usage":{"limit":"100","used":"101"}}"#,
                r#"{"usage":{"limit":"0","used":"0"}}"#,
                r#"{"usage":{"limit":"100"}}"#,
            ] {
                assert!(parse(id, body.as_bytes(), "now").is_err());
            }
            for count in ["", "NaN", "inf", "-1", "1e2", " 20", "true"] {
                let body = serde_json::json!({"usage":{"limit":"100","used":count}});
                assert!(parse(id, body.to_string().as_bytes(), "now").is_err());
            }
        }
    }
    #[test]
    fn wallets_keep_currency_and_negative_cash_separate_from_quota() {
        for (id, unit) in [
            ("kimi_api_cn", "currency_unknown"),
            ("kimi_api_intl", "USD"),
        ] {
            let m = parse(
                id,
                include_bytes!("../../../fixtures/providers/moonshot-balance.json"),
                "now",
            )
            .unwrap();
            assert_eq!(m.len(), 3);
            assert_eq!(m[0].unit, unit);
            assert!(m
                .iter()
                .all(|m| m.kind == "balance" && m.remaining_percent.is_none()));
            assert!(m[2].remaining.as_ref().unwrap().starts_with("-2.5"));
        }
        for body in [
            r#"{"code":0,"data":{}}"#,
            r#"{"code":0,"status":false,"data":{"available_balance":99}}"#,
            r#"{"code":0,"data":{"available_balance":"NaN"}}"#,
        ] {
            assert!(parse("kimi_api_cn", body.as_bytes(), "now").is_err());
        }
    }
    #[test]
    fn fixtures_and_regions_preserve_quota_direction() {
        for id in ["glm_coding_cn", "glm_coding_intl"] {
            let m = parse(
                id,
                include_bytes!("../../../fixtures/providers/glm-coding.json"),
                "now",
            )
            .unwrap();
            assert_eq!(m[0].remaining_percent, Some(75.0));
            assert_eq!(m[1].remaining_percent, Some(0.0));
            assert!(m[0].resets_at.is_none());
        }
        for id in ["kimi_coding_cn", "kimi_coding_intl"] {
            let m = parse(
                id,
                include_bytes!("../../../fixtures/providers/kimi-coding.json"),
                "now",
            )
            .unwrap();
            assert_eq!(m[0].remaining_percent, Some(80.0));
            assert_eq!(m[1].remaining_percent, Some(0.0));
            assert!(m[0].resets_at.is_some());
        }
    }
    #[test]
    fn reject_missing_invalid_and_cross_region_addresses() {
        for id in [
            "glm_coding_cn",
            "glm_coding_intl",
            "kimi_coding_cn",
            "kimi_coding_intl",
        ] {
            assert!(parse(id, b"{}", "now").is_err());
            let p = RegionalProvider(id);
            let url = p.descriptor().base_urls[0];
            assert!(p.validate_url(url).is_ok());
            for evil in [
                format!("{url}.evil.test"),
                format!("{url}/redirect"),
                "http://localhost".into(),
            ] {
                assert!(p.validate_url(&evil).is_err());
            }
        }
        for data in [
            r#"{"usage":{"limit":0,"used":0}}"#,
            r#"{"usage":{"limit":100}}"#,
            r#"{"usage":{"limit":100,"used":101}}"#,
            r#"{"usage":{"limit":100,"used":20,"remaining":90}}"#,
            r#"{"usage":{"limit":100,"used":20,"resetTime":"bad"}}"#,
        ] {
            assert!(parse("kimi_coding_cn", data.as_bytes(), "now").is_err());
        }
        for value in [-1, 101] {
            let body = serde_json::json!({"success":true,"data":{"limits":[{"type":"TOKENS_LIMIT","percentage":value}]}});
            assert!(parse("glm_coding_cn", body.to_string().as_bytes(), "now").is_err());
        }
    }
    #[test]
    fn minimax_international_reuses_only_matching_product_parser() {
        for (id, body) in [
            (
                "minimax_api_intl",
                include_bytes!("../../../fixtures/providers/minimax-balance.json").as_slice(),
            ),
            (
                "minimax_token_plan_intl",
                include_bytes!("../../../fixtures/providers/minimax-token-plan.json").as_slice(),
            ),
        ] {
            assert!(super::super::plans::parse(id, body, "now").is_ok());
            let p = super::super::plans::PlanProvider(id);
            assert!(p.validate_url(p.descriptor().base_urls[0]).is_ok());
            assert!(p.validate_url("https://www.minimaxi.com").is_err());
        }
    }
}
