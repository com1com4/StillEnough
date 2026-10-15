use crate::domain::models::{AlertEvent, QuotaMetric};
use rusqlite::{params, OptionalExtension};

/// Percent-of-remaining thresholds, most severe last. Only metrics with a real
/// remaining percentage are eligible; balance metrics have no percentage to compare.
pub const DEFAULT_THRESHOLDS: [f64; 4] = [30.0, 20.0, 10.0, 5.0];
/// Consecutive valid observations back above `threshold + 5` needed to re-arm.
const REARM_MARGIN: f64 = 5.0;
const REARM_OBSERVATIONS: i64 = 2;

pub fn sanitize_thresholds(values: &[f64]) -> Vec<f64> {
    let mut cleaned: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 100.0)
        .map(|value| (value * 10.0).round() / 10.0)
        .collect();
    cleaned.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    cleaned.dedup();
    if cleaned.is_empty() || cleaned.len() > 6 {
        return DEFAULT_THRESHOLDS.to_vec();
    }
    cleaned
}

/// Fixed windows anchor on the reset instant the source reports; a few minutes of
/// prediction drift inside the same hour must not look like a new period. Metrics
/// without a reset anchor use a persistent episode instead.
pub fn period_key(metric: &QuotaMetric) -> String {
    match metric
        .resets_at
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
    {
        Some(reset) => {
            let hour = reset
                .with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H")
                .to_string();
            format!("fixed:{hour}")
        }
        None => "rolling".to_string(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Armed {
    period_key: String,
    armed_threshold: f64,
    armed: bool,
    recoveries: i64,
    /// Bumped every time the metric becomes armed again, so a later episode inside the same
    /// period produces a new dedupe key instead of colliding with the first one.
    episode: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub state: Armed,
    /// Present when this observation produced a new alert event.
    pub event: Option<Trigger>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub threshold: f64,
    pub crossed: Vec<f64>,
    pub remaining_percent: f64,
    pub period_key: String,
}

impl Armed {
    fn initial(period_key: &str, thresholds: &[f64]) -> Self {
        Self {
            period_key: period_key.to_string(),
            armed_threshold: thresholds[0],
            armed: true,
            recoveries: 0,
            episode: 1,
        }
    }
}

/// Pure state machine for one metric. `previous` is None the first time a metric is seen.
pub fn decide(
    previous: Option<&Armed>,
    metric: &QuotaMetric,
    remaining: f64,
    thresholds: &[f64],
    reset_passed: bool,
) -> Decision {
    let period_key = period_key(metric);
    let Some(previous) = previous else {
        // First observation only arms; it cannot prove a crossing happened.
        return Decision {
            state: Armed::initial(&period_key, thresholds),
            event: None,
        };
    };
    if previous.period_key != period_key {
        // A new period starts unarmed-free: the next drop below a threshold alerts again.
        return Decision {
            state: Armed::initial(&period_key, thresholds),
            event: None,
        };
    }
    let mut state = previous.clone();
    if reset_passed {
        // The value describes a window that already ended; do not judge it.
        state.recoveries = 0;
        return Decision { state, event: None };
    }
    if state.armed {
        let crossed: Vec<f64> = thresholds
            .iter()
            .copied()
            .filter(|threshold| remaining <= *threshold)
            .collect();
        if let Some(threshold) = crossed.last().copied() {
            // One event per observation: the most severe crossed threshold, all crossings recorded.
            state.armed = false;
            state.recoveries = 0;
            state.armed_threshold = threshold;
            return Decision {
                state,
                event: Some(Trigger {
                    threshold,
                    crossed,
                    remaining_percent: remaining,
                    period_key,
                }),
            };
        }
        state.recoveries = 0;
        return Decision { state, event: None };
    }
    if remaining >= state.armed_threshold + REARM_MARGIN {
        state.recoveries += 1;
        if state.recoveries >= REARM_OBSERVATIONS {
            state.armed = true;
            state.recoveries = 0;
            state.armed_threshold = thresholds[0];
            state.episode += 1;
        }
    } else if remaining <= state.armed_threshold {
        state.recoveries = 0;
    } else {
        // Inside the hysteresis band: still below the re-arm margin, so re-arm progress resets.
        state.recoveries = 0;
    }
    Decision { state, event: None }
}

fn load_state(
    db: &rusqlite::Connection,
    account_id: &str,
    metric_id: &str,
) -> Result<Option<Armed>, String> {
    db.query_row(
        "SELECT period_key,armed_threshold,armed,recoveries,episode FROM alert_state WHERE account_id=?1 AND metric_id=?2",
        params![account_id, metric_id],
        |row| {
            Ok(Armed {
                period_key: row.get(0)?,
                armed_threshold: row.get(1)?,
                armed: row.get(2)?,
                recoveries: row.get(3)?,
                episode: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|error| format!("告警状态读取失败：{error}"))
}

fn save_state(
    db: &rusqlite::Connection,
    account_id: &str,
    metric: &QuotaMetric,
    state: &Armed,
) -> Result<(), String> {
    db.execute(
        "INSERT INTO alert_state(account_id,metric_id,period_key,armed_threshold,armed,recoveries,episode,updated_at) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(account_id,metric_id) DO UPDATE SET \
         period_key=excluded.period_key,armed_threshold=excluded.armed_threshold,armed=excluded.armed,recoveries=excluded.recoveries,episode=excluded.episode,updated_at=excluded.updated_at",
        params![
            account_id,
            metric.metric_id,
            state.period_key,
            state.armed_threshold,
            state.armed,
            state.recoveries,
            state.episode,
            metric.observed_at
        ],
    )
    .map_err(|error| format!("告警状态保存失败：{error}"))?;
    Ok(())
}

/// Evaluate freshly written metrics inside the observation transaction.
/// Returns only the events this call actually inserted, so restarts never replay them.
pub fn evaluate(
    db: &rusqlite::Connection,
    account_id: &str,
    metrics: &[QuotaMetric],
    enabled: bool,
    thresholds: &[f64],
) -> Result<Vec<i64>, String> {
    if !enabled || thresholds.is_empty() {
        return Ok(Vec::new());
    }
    let now = chrono::Utc::now();
    let mut created = Vec::new();
    for metric in metrics {
        // Only quota windows with a real remaining percentage can be compared to a threshold.
        if metric.kind != "quota"
            || !matches!(metric.source.as_str(), "official_api" | "client_endpoint")
        {
            continue;
        }
        let Some(remaining) = metric.remaining_percent else {
            continue;
        };
        let reset_passed = metric
            .resets_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|reset| reset < now);
        let previous = load_state(db, account_id, &metric.metric_id)?;
        let decision = decide(
            previous.as_ref(),
            metric,
            remaining,
            thresholds,
            reset_passed,
        );
        save_state(db, account_id, metric, &decision.state)?;
        let Some(trigger) = decision.event else {
            continue;
        };
        let dedupe_key = format!(
            "{}|{}|{:.1}|{}|{}",
            account_id,
            metric.metric_id,
            trigger.threshold,
            trigger.period_key,
            decision.state.episode
        );
        let crossed = serde_json::to_string(&trigger.crossed).map_err(|_| "告警阈值保存失败")?;
        let inserted = db
            .execute(
                "INSERT OR IGNORE INTO alert_events(account_id,metric_id,metric_label,kind,unit,threshold,crossed_thresholds,remaining_percent,period_key,dedupe_key,triggered_at) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    account_id,
                    metric.metric_id,
                    metric.label,
                    metric.kind,
                    metric.unit,
                    trigger.threshold,
                    crossed,
                    trigger.remaining_percent,
                    trigger.period_key,
                    dedupe_key,
                    metric.observed_at
                ],
            )
            .map_err(|error| format!("告警记录失败：{error}"))?;
        if inserted == 1 {
            created.push(db.last_insert_rowid());
        }
    }
    Ok(created)
}

pub fn list(
    db: &rusqlite::Connection,
    account_id: Option<&str>,
    limit: usize,
) -> Result<Vec<AlertEvent>, String> {
    let limit = limit.clamp(1, 200) as i64;
    let mut statement = db
        .prepare(
            "SELECT e.id,e.account_id,COALESCE(a.alias,''),COALESCE(a.product_name,''),e.metric_id,e.metric_label,e.kind,e.unit,\
             e.threshold,e.crossed_thresholds,e.remaining_percent,e.period_key,e.triggered_at,e.read_at \
             FROM alert_events e LEFT JOIN accounts a ON a.id=e.account_id \
             WHERE (?1 IS NULL OR e.account_id=?1) ORDER BY e.triggered_at DESC,e.id DESC LIMIT ?2",
        )
        .map_err(|error| format!("告警读取失败：{error}"))?;
    let rows = statement
        .query_map(params![account_id, limit], |row| {
            let crossed: String = row.get(9)?;
            Ok(AlertEvent {
                id: row.get(0)?,
                account_id: row.get(1)?,
                alias: row.get(2)?,
                product_name: row.get(3)?,
                metric_id: row.get(4)?,
                metric_label: row.get(5)?,
                kind: row.get(6)?,
                unit: row.get(7)?,
                threshold: row.get(8)?,
                crossed_thresholds: serde_json::from_str(&crossed).unwrap_or_default(),
                remaining_percent: row.get(10)?,
                period_key: row.get(11)?,
                triggered_at: row.get(12)?,
                read_at: row.get(13)?,
            })
        })
        .map_err(|error| format!("告警读取失败：{error}"))?;
    rows.map(|row| row.map_err(|error| format!("告警读取失败：{error}")))
        .collect()
}

pub fn unread_count(db: &rusqlite::Connection) -> Result<i64, String> {
    db.query_row(
        "SELECT COUNT(*) FROM alert_events WHERE read_at IS NULL",
        [],
        |row| row.get(0),
    )
    .map_err(|error| format!("告警计数失败：{error}"))
}

pub fn mark_read(db: &rusqlite::Connection, id: i64) -> Result<bool, String> {
    db.execute(
        "UPDATE alert_events SET read_at=?2 WHERE id=?1 AND read_at IS NULL",
        params![id, chrono::Utc::now().to_rfc3339()],
    )
    .map(|count| count > 0)
    .map_err(|error| format!("告警更新失败：{error}"))
}

pub fn mark_all_read(db: &rusqlite::Connection) -> Result<usize, String> {
    db.execute(
        "UPDATE alert_events SET read_at=?1 WHERE read_at IS NULL",
        [chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|error| format!("告警更新失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn metric(percent: f64, resets_at: Option<&str>, observed_at: &str) -> QuotaMetric {
        QuotaMetric {
            metric_id: "quota_5h".into(),
            label: "5 小时窗口".into(),
            kind: "quota".into(),
            unit: "%".into(),
            remaining: Some(format!("{percent:.1}%")),
            limit: None,
            remaining_percent: Some(percent),
            resets_at: resets_at.map(|v| v.into()),
            source: "official_api".into(),
            observed_at: observed_at.into(),
            stale_after_seconds: 1200,
        }
    }

    fn decisions(percentages: &[f64]) -> Vec<Option<Trigger>> {
        let mut state: Option<Armed> = None;
        let mut events = Vec::new();
        for percent in percentages {
            let decision = decide(
                state.as_ref(),
                &metric(*percent, None, "2026-10-01T00:00:00Z"),
                *percent,
                &DEFAULT_THRESHOLDS,
                false,
            );
            state = Some(decision.state);
            events.push(decision.event);
        }
        events
    }

    #[test]
    fn first_observation_only_arms_and_a_single_drop_fires_the_most_severe_threshold() {
        let events = decisions(&[40.0, 5.0]);
        assert!(events[0].is_none());
        let event = events[1].clone().expect("5% 必须触发");
        assert_eq!(event.threshold, 5.0);
        assert_eq!(event.crossed, vec![30.0, 20.0, 10.0, 5.0]);
    }

    #[test]
    fn a_single_observation_jumping_from_thirty_to_five_alerts_once() {
        let events = decisions(&[31.0, 5.0, 4.0, 3.0]);
        assert!(events[0].is_none());
        assert_eq!(events[1].as_ref().unwrap().threshold, 5.0);
        assert!(events[2].is_none(), "同一 episode 只提醒一次");
        assert!(events[3].is_none());
    }

    #[test]
    fn rearm_needs_two_valid_observations_above_the_margin() {
        // 10% fires; 12%/13% stay inside the hysteresis band, 16%/16% clear the margin.
        let events = decisions(&[50.0, 10.0, 12.0, 13.0, 16.0, 16.0, 10.0]);
        assert_eq!(events[1].as_ref().unwrap().threshold, 10.0);
        assert!(events[2].is_none() && events[3].is_none() && events[4].is_none());
        assert!(events[5].is_none(), "第一次回到阈值以上不重新布防");
        assert_eq!(events[6].as_ref().map(|e| e.threshold), Some(10.0));
    }

    #[test]
    fn a_new_fixed_period_arms_again_without_help_from_time_alone() {
        let mut state: Option<Armed> = None;
        let mut fired = 0;
        // Same window, drift of the reported reset by minutes: still one period.
        for (index, reset) in [
            "2026-10-01T05:00:00Z",
            "2026-10-01T05:07:00Z",
            "2026-10-01T05:03:00Z",
        ]
        .iter()
        .enumerate()
        {
            let percent = if index == 0 { 50.0 } else { 4.0 };
            let decision = decide(
                state.as_ref(),
                &metric(percent, Some(reset), "2026-10-01T00:00:00Z"),
                percent,
                &DEFAULT_THRESHOLDS,
                false,
            );
            state = Some(decision.state);
            fired += usize::from(decision.event.is_some());
        }
        assert_eq!(fired, 1, "重置时间小幅漂移不应产生新周期");
        let next = decide(
            state.as_ref(),
            &metric(4.0, Some("2026-10-02T05:00:00Z"), "2026-10-02T00:00:00Z"),
            4.0,
            &DEFAULT_THRESHOLDS,
            false,
        );
        assert!(next.event.is_none(), "新周期首次观测只布防");
        let again = decide(
            Some(&next.state),
            &metric(4.0, Some("2026-10-02T05:00:00Z"), "2026-10-02T00:10:00Z"),
            4.0,
            &DEFAULT_THRESHOLDS,
            false,
        );
        assert!(again.event.is_some(), "下个周期可以再次提醒");
    }

    #[test]
    fn thresholds_are_sorted_deduplicated_and_bounded() {
        assert_eq!(
            sanitize_thresholds(&[20.0, 5.0, 5.0, 250.0, -1.0, 30.0]),
            vec![30.0, 20.0, 5.0]
        );
        assert_eq!(sanitize_thresholds(&[]), DEFAULT_THRESHOLDS.to_vec());
        assert_eq!(
            sanitize_thresholds(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]),
            DEFAULT_THRESHOLDS.to_vec()
        );
    }

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
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

    #[test]
    fn balance_and_unknown_metrics_never_alert_and_state_survives_reopen() {
        let db = database();
        let mut balance = metric(0.0, None, "2026-10-01T00:00:00Z");
        balance.kind = "balance".into();
        balance.remaining_percent = None;
        assert!(evaluate(&db, "a", &[balance], true, &DEFAULT_THRESHOLDS)
            .unwrap()
            .is_empty());
        let mut unknown = metric(50.0, None, "2026-10-01T00:00:00Z");
        unknown.remaining_percent = None;
        assert!(evaluate(&db, "a", &[unknown], true, &DEFAULT_THRESHOLDS)
            .unwrap()
            .is_empty());
        assert!(evaluate(&db, "a", &[], true, &DEFAULT_THRESHOLDS)
            .unwrap()
            .is_empty());

        evaluate(
            &db,
            "a",
            &[metric(50.0, None, "2026-10-01T00:00:00Z")],
            true,
            &DEFAULT_THRESHOLDS,
        )
        .unwrap();
        let first = evaluate(
            &db,
            "a",
            &[metric(4.0, None, "2026-10-01T00:20:00Z")],
            true,
            &DEFAULT_THRESHOLDS,
        )
        .unwrap();
        assert_eq!(first.len(), 1);
        // Restart: the same observation must not produce a second event.
        let replay = evaluate(
            &db,
            "a",
            &[metric(4.0, None, "2026-10-01T00:20:00Z")],
            true,
            &DEFAULT_THRESHOLDS,
        )
        .unwrap();
        assert!(replay.is_empty());
        assert_eq!(unread_count(&db).unwrap(), 1);

        let events = list(&db, Some("a"), 50).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].alias, "别名");
        assert_eq!(events[0].crossed_thresholds, vec![30.0, 20.0, 10.0, 5.0]);
        assert!(events[0].read_at.is_none());
        assert!(mark_read(&db, events[0].id).unwrap());
        assert!(!mark_read(&db, events[0].id).unwrap());
        assert_eq!(unread_count(&db).unwrap(), 0);
    }

    #[test]
    fn disabled_alerting_records_no_events_and_rolling_episodes_rearm_after_recovery() {
        let db = database();
        assert!(evaluate(
            &db,
            "a",
            &[metric(2.0, None, "2026-10-01T00:00:00Z")],
            false,
            &DEFAULT_THRESHOLDS
        )
        .unwrap()
        .is_empty());
        evaluate(
            &db,
            "a",
            &[metric(50.0, None, "2026-10-01T01:00:00Z")],
            true,
            &DEFAULT_THRESHOLDS,
        )
        .unwrap();
        let first = evaluate(
            &db,
            "a",
            &[metric(2.0, None, "2026-10-01T01:10:00Z")],
            true,
            &DEFAULT_THRESHOLDS,
        )
        .unwrap();
        assert_eq!(first.len(), 1);
        for percent in [12.0, 20.0] {
            assert!(evaluate(
                &db,
                "a",
                &[metric(percent, None, "2026-10-01T01:20:00Z")],
                true,
                &DEFAULT_THRESHOLDS
            )
            .unwrap()
            .is_empty());
        }
        let second = evaluate(
            &db,
            "a",
            &[metric(2.0, None, "2026-10-01T01:30:00Z")],
            true,
            &DEFAULT_THRESHOLDS,
        )
        .unwrap();
        assert_eq!(second.len(), 1, "rolling 恢复到阈值+5 以上两次后再次提醒");
        assert_eq!(list(&db, None, 50).unwrap().len(), 2);
        assert_eq!(mark_all_read(&db).unwrap(), 2);
    }
}
