use crate::providers::ProviderProjection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const SCHEMA_VERSION: u8 = 1;
pub const PROTOCOL_VERSION: &str = "1.0";
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    Unknown,
    Expired,
    AuthRequired,
    Forbidden,
    ApiChanged,
    RateLimited,
    NetworkError,
    Unsupported,
    Busy,
    Paused,
    Error,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Freshness {
    Current,
    Unknown,
    Expired,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub schema_version: u8,
    pub request_id: String,
    pub command: String,
    pub captured_at: String,
    pub transport: String,
    pub instance_id: Option<String>,
    pub revision: Option<u64>,
    pub outcome: String,
    pub data: Data,
    pub errors: Vec<SafeError>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub kind: String,
    pub accounts: Vec<Account>,
    pub providers: Vec<ProviderProjection>,
    pub checks: Vec<Check>,
    pub operation_id: Option<String>,
    pub operation_state: Option<String>,
    pub metadata: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub provider_id: String,
    pub alias: String,
    pub enabled: bool,
    pub status: Status,
    pub error_code: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub refreshing: bool,
    pub metrics: Vec<Metric>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub metric_id: String,
    pub label: String,
    pub kind: MetricKind,
    pub unit: String,
    pub remaining: Option<String>,
    pub limit: Option<String>,
    #[serde(serialize_with = "serialize_percent")]
    pub remaining_percent: Percent,
    pub resets_at: Option<String>,
    pub source: String,
    pub observed_at: String,
    pub stale_after_seconds: u64,
    pub freshness: Freshness,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricKind {
    Quota,
    Balance,
}
/// 构造时区分钱包与额度；钱包永远不产生百分比。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Percent(Option<f64>);
impl Percent {
    pub fn new(kind: &MetricKind, value: Option<f64>) -> Self {
        Self(match kind {
            MetricKind::Balance => None,
            MetricKind::Quota => value.filter(|v| v.is_finite() && (0.0..=100.0).contains(v)),
        })
    }
}
fn serialize_percent<S: serde::Serializer>(p: &Percent, s: S) -> Result<S::Ok, S::Error> {
    p.0.serialize(s)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub state: String,
    pub code: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeError {
    pub code: String,
    pub status: Status,
    pub scope: String,
    pub account_id: Option<String>,
    pub retryable: bool,
    pub next_retry_at: Option<String>,
    pub message: String,
}
impl Response {
    pub fn local(command: &str, kind: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            request_id: uuid::Uuid::new_v4().to_string(),
            command: command.into(),
            captured_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            transport: "local".into(),
            instance_id: None,
            revision: None,
            outcome: "ok".into(),
            data: Data {
                kind: kind.into(),
                accounts: vec![],
                providers: vec![],
                checks: vec![],
                operation_id: None,
                operation_state: None,
                metadata: BTreeMap::new(),
            },
            errors: vec![],
            warnings: vec![],
        }
    }
    pub fn invalid_arguments() -> Self {
        let mut r = Self::local("invalid", "version");
        r.outcome = "error".into();
        r.errors.push(SafeError {
            code: "invalid_arguments".into(),
            status: Status::Error,
            scope: "request".into(),
            account_id: None,
            retryable: false,
            next_retry_at: None,
            message: "参数无效；使用 --help 查看用法".into(),
        });
        r
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentages_preserve_unknown_and_zero() {
        for (kind, value, expected) in [
            (MetricKind::Balance, Some(100.0), None),
            (MetricKind::Quota, None, None),
            (MetricKind::Quota, Some(0.0), Some(0.0)),
            (MetricKind::Quota, Some(f64::NAN), None),
        ] {
            assert_eq!(Percent::new(&kind, value).0, expected);
        }
    }
    #[test]
    fn statuses_are_independent_of_freshness() {
        assert_eq!(
            serde_json::to_string(&Status::AuthRequired).unwrap(),
            "\"auth-required\""
        );
        assert_eq!(
            serde_json::to_string(&Freshness::Expired).unwrap(),
            "\"expired\""
        );
    }
}
