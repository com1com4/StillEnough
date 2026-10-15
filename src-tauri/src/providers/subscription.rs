use super::{FetchError, QuotaMetric};
use serde_json::Value;

pub fn window(
    id: String,
    label: String,
    used: &Value,
    reset: Option<String>,
    now: &str,
) -> Result<QuotaMetric, FetchError> {
    let used = used
        .as_f64()
        .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))
        .ok_or_else(|| FetchError::new("schema_changed"))?;
    Ok(QuotaMetric {
        metric_id: id,
        label,
        kind: "quota".into(),
        unit: "percent".into(),
        remaining: None,
        limit: None,
        remaining_percent: Some(100.0 - used),
        resets_at: reset,
        source: "client_endpoint".into(),
        observed_at: now.into(),
        stale_after_seconds: 1200,
    })
}
pub fn unix_reset(value: &Value) -> Result<Option<String>, FetchError> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_i64()
        .and_then(|n| chrono::DateTime::from_timestamp(n, 0))
        .map(|d| Some(d.to_rfc3339()))
        .ok_or_else(|| FetchError::new("schema_changed"))
}
pub fn iso_reset(value: &Value) -> Result<Option<String>, FetchError> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| Some(d.to_rfc3339()))
        .ok_or_else(|| FetchError::new("schema_changed"))
}
