use crate::domain::models::{HistoryPoint, HistorySegment, HistorySeries, QuotaMetric};
use rusqlite::params;

pub const DEFAULT_RETENTION_DAYS: u32 = 90;
pub const ALLOWED_RETENTION_DAYS: [u32; 3] = [30, 60, 90];
/// A silent window longer than this ends the line instead of interpolating across it.
const MIN_GAP_SECONDS: i64 = 2 * 3600;
const MAX_POINTS_PER_SEGMENT: usize = 360;

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub observed_at: String,
    pub resets_at: Option<String>,
    pub value: Option<f64>,
    pub text: Option<String>,
}

/// Balance observations keep the provider's own text ("12.34 USD"); the chart needs a
/// number. Only a plain decimal carrying the metric's own unit is accepted.
pub fn balance_number(remaining: &str, unit: &str) -> Option<f64> {
    let number = remaining
        .strip_suffix(&format!(" {unit}"))
        .unwrap_or(remaining)
        .trim();
    if number.is_empty()
        || number.split('.').count() > 2
        || !number
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    number.parse::<f64>().ok()
}

fn epoch(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp())
}

/// Split samples into contiguous segments. A different reset anchor or a long silent gap
/// starts a new segment, so the chart never draws a line across a reset or a data outage.
pub fn segment(samples: &[Sample], gap_seconds: i64) -> Vec<HistorySegment> {
    let gap = gap_seconds.max(MIN_GAP_SECONDS);
    let mut segments: Vec<HistorySegment> = Vec::new();
    let mut current: Vec<&Sample> = Vec::new();
    for sample in samples {
        let starts_segment = current.last().is_some_and(|previous| {
            previous.resets_at != sample.resets_at
                || match (epoch(&previous.observed_at), epoch(&sample.observed_at)) {
                    (Some(a), Some(b)) => b - a > gap,
                    _ => true,
                }
        });
        if starts_segment {
            segments.push(to_segment(&current));
            current.clear();
        }
        current.push(sample);
    }
    if !current.is_empty() {
        segments.push(to_segment(&current));
    }
    segments
}

fn to_segment(samples: &[&Sample]) -> HistorySegment {
    // Keep the newest point and thin the rest so a 30 day window stays drawable.
    let step = (samples.len() / MAX_POINTS_PER_SEGMENT).max(1);
    let points = samples
        .iter()
        .enumerate()
        .filter(|(index, _)| index % step == 0 || *index + 1 == samples.len())
        .map(|(_, sample)| HistoryPoint {
            observed_at: sample.observed_at.clone(),
            value: sample.value,
            text: sample.text.clone(),
        })
        .collect();
    HistorySegment {
        resets_at: samples.first().and_then(|first| first.resets_at.clone()),
        points,
    }
}

pub struct Query {
    pub days: u32,
    pub account_id: Option<String>,
    pub gap_seconds: i64,
}

pub fn list(db: &rusqlite::Connection, query: &Query) -> Result<Vec<HistorySeries>, String> {
    let days = query.days.clamp(1, DEFAULT_RETENTION_DAYS);
    let since = (chrono::Utc::now() - chrono::Duration::days(i64::from(days))).to_rfc3339();
    let mut statement = db
        .prepare(
            "SELECT o.account_id,COALESCE(a.alias,''),COALESCE(a.product_name,''),o.observed_at,o.metrics_json \
             FROM observations o JOIN accounts a ON a.id=o.account_id \
             WHERE o.observed_at>=?1 AND (?2 IS NULL OR o.account_id=?2) \
             ORDER BY o.account_id,o.observed_at",
        )
        .map_err(|error| format!("历史读取失败：{error}"))?;
    let rows = statement
        .query_map(params![since, query.account_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| format!("历史读取失败：{error}"))?;
    let mut series: Vec<HistorySeries> = Vec::new();
    let mut samples: Vec<Vec<Sample>> = Vec::new();
    for row in rows {
        let (account_id, alias, product_name, observed_at, payload) =
            row.map_err(|error| format!("历史读取失败：{error}"))?;
        let metrics: Vec<QuotaMetric> =
            serde_json::from_str(&payload).map_err(|_| "历史记录无法解析".to_string())?;
        for metric in metrics {
            let value = match metric.kind.as_str() {
                "quota" => metric.remaining_percent,
                _ => metric
                    .remaining
                    .as_deref()
                    .and_then(|text| balance_number(text, &metric.unit)),
            };
            // A metric with no readable value contributes no point; it must not draw a zero.
            let Some(value) = value else {
                continue;
            };
            let index = match series.iter().position(|entry| {
                entry.account_id == account_id && entry.metric_id == metric.metric_id
            }) {
                Some(index) => index,
                None => {
                    series.push(HistorySeries {
                        account_id: account_id.clone(),
                        alias: alias.clone(),
                        product_name: product_name.clone(),
                        metric_id: metric.metric_id.clone(),
                        metric_label: metric.label.clone(),
                        kind: metric.kind.clone(),
                        unit: metric.unit.clone(),
                        segments: Vec::new(),
                        observed_count: 0,
                    });
                    samples.push(Vec::new());
                    series.len() - 1
                }
            };
            series[index].observed_count += 1;
            samples[index].push(Sample {
                observed_at: observed_at.clone(),
                resets_at: metric.resets_at,
                value: Some(value),
                text: metric.remaining,
            });
        }
    }
    for (entry, collected) in series.iter_mut().zip(samples) {
        entry.segments = segment(&collected, query.gap_seconds);
    }
    Ok(series)
}

pub fn prune(db: &rusqlite::Connection, retention_days: u32) -> Result<usize, String> {
    let days = if ALLOWED_RETENTION_DAYS.contains(&retention_days) {
        retention_days
    } else {
        DEFAULT_RETENTION_DAYS
    };
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(i64::from(days))).to_rfc3339();
    db.execute("DELETE FROM observations WHERE observed_at<?1", [&cutoff])
        .map_err(|error| format!("历史清理失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(observed_at: &str, resets_at: Option<&str>, value: f64) -> Sample {
        Sample {
            observed_at: observed_at.into(),
            resets_at: resets_at.map(|v| v.into()),
            value: Some(value),
            text: None,
        }
    }

    #[test]
    fn a_reset_anchor_and_a_long_gap_each_start_a_new_segment() {
        let samples = vec![
            sample("2026-10-01T00:00:00Z", Some("2026-10-01T05:00:00Z"), 40.0),
            sample("2026-10-01T01:00:00Z", Some("2026-10-01T05:00:00Z"), 30.0),
            sample("2026-10-01T02:00:00Z", Some("2026-10-01T06:00:00Z"), 20.0),
            sample("2026-10-01T09:00:00Z", Some("2026-10-01T06:00:00Z"), 10.0),
        ];
        let segments = segment(&samples, 7200);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].points.len(), 2);
        assert_eq!(segments[1].points.len(), 1);
        assert_eq!(
            segments[2].resets_at.as_deref(),
            Some("2026-10-01T06:00:00Z")
        );
    }

    #[test]
    fn balance_text_is_only_read_as_a_number_when_it_matches_its_unit() {
        assert_eq!(balance_number("12.34 USD", "USD"), Some(12.34));
        assert_eq!(balance_number("0 CNY", "CNY"), Some(0.0));
        assert_eq!(balance_number("12.34 USD", "CNY"), None);
        assert_eq!(balance_number("未知", "USD"), None);
        assert_eq!(balance_number("-1.5 USD", "USD"), None);
        assert_eq!(balance_number("", "USD"), None);
    }

    fn database() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.pragma_update(None, "foreign_keys", "ON").unwrap();
        db.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/002_quota_only.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/003_connections.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/004_cli_authorization.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/005_monitoring.sql"))
            .unwrap();
        db.execute(
            "INSERT INTO accounts VALUES('a','deepseek_api','别名','DeepSeek API',1,'now','now')",
            [],
        )
        .unwrap();
        db
    }

    fn observation(db: &rusqlite::Connection, observed_at: &str, json: &str) {
        db.execute(
            "INSERT INTO observations(account_id,observed_at,metrics_json) VALUES('a',?1,?2)",
            params![observed_at, json],
        )
        .unwrap();
    }

    fn metric_json(
        metric_id: &str,
        kind: &str,
        unit: &str,
        percent: Option<f64>,
        text: &str,
        resets_at: Option<&str>,
    ) -> String {
        serde_json::json!([{
            "metricId": metric_id,
            "label": metric_id,
            "kind": kind,
            "unit": unit,
            "remaining": text,
            "limit": null,
            "remainingPercent": percent,
            "resetsAt": resets_at,
            "source": "official_api",
            "observedAt": "2026-10-01T00:00:00Z"
        }])
        .to_string()
    }

    #[test]
    fn history_reports_quota_and_balance_series_and_skips_unknown_values() {
        let db = database();
        let now = chrono::Utc::now();
        observation(
            &db,
            &(now - chrono::Duration::hours(3)).to_rfc3339(),
            &metric_json(
                "5 小时窗口",
                "quota",
                "%",
                Some(40.0),
                "40.0%",
                Some("2026-10-02T00:00:00Z"),
            ),
        );
        observation(
            &db,
            &(now - chrono::Duration::hours(2)).to_rfc3339(),
            &metric_json(
                "5 小时窗口",
                "quota",
                "%",
                Some(35.0),
                "35.0%",
                Some("2026-10-02T00:00:00Z"),
            ),
        );
        observation(
            &db,
            &(now - chrono::Duration::hours(2)).to_rfc3339(),
            &metric_json("可用余额 · USD", "balance", "USD", None, "12.50 USD", None),
        );
        observation(
            &db,
            &(now - chrono::Duration::hours(1)).to_rfc3339(),
            &metric_json("无数据窗口", "quota", "%", None, "未知", None),
        );
        let series = list(
            &db,
            &Query {
                days: 7,
                account_id: None,
                gap_seconds: 7200,
            },
        )
        .unwrap();
        assert_eq!(series.len(), 2, "未知额度不生成曲线点");
        let quota = series.iter().find(|s| s.kind == "quota").unwrap();
        assert_eq!(quota.alias, "别名");
        assert_eq!(quota.observed_count, 2);
        assert_eq!(quota.segments[0].points[0].value, Some(40.0));
        let balance = series.iter().find(|s| s.kind == "balance").unwrap();
        assert_eq!(balance.segments[0].points[0].value, Some(12.5));
        assert_eq!(
            balance.segments[0].points[0].text.as_deref(),
            Some("12.50 USD")
        );

        let scoped = list(
            &db,
            &Query {
                days: 7,
                account_id: Some("missing".into()),
                gap_seconds: 7200,
            },
        )
        .unwrap();
        assert!(scoped.is_empty());
    }

    #[test]
    fn retention_prunes_only_observations_older_than_the_window() {
        let db = database();
        let now = chrono::Utc::now();
        observation(&db, &(now - chrono::Duration::days(120)).to_rfc3339(), "[]");
        observation(&db, &(now - chrono::Duration::days(10)).to_rfc3339(), "[]");
        assert_eq!(prune(&db, 90).unwrap(), 1);
        assert_eq!(prune(&db, 30).unwrap(), 0);
        observation(&db, &(now - chrono::Duration::days(40)).to_rfc3339(), "[]");
        assert_eq!(prune(&db, 30).unwrap(), 1);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM observations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
