use crate::{
    access, errors,
    protocol::{Account, Check, Freshness, Metric, MetricKind, Percent, Response, Status},
    request::{Command, Request},
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use std::{path::Path, time::Duration};
pub const DATABASE_SCHEMA: i64 = 6;
pub fn schema(db: &Connection) -> Result<(), &'static str> {
    let max: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "migration_required")?;
    if max > DATABASE_SCHEMA {
        return Err("schema_incompatible");
    }
    if max < DATABASE_SCHEMA {
        return Err("migration_required");
    }
    let count: i64 = db
        .query_row(
            "SELECT COUNT(DISTINCT version) FROM schema_migrations WHERE version BETWEEN 1 AND 6",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "database_error")?;
    if count != 6 {
        return Err("migration_required");
    }
    Ok(())
}
fn sql_error(e: rusqlite::Error) -> &'static str {
    match e {
        rusqlite::Error::SqliteFailure(ref err, _)
            if matches!(
                err.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            "database_busy"
        }
        _ => "database_error",
    }
}
pub fn offline(path: &Path, request: &Request) -> Response {
    let mut r = Response::local(request.command.name(), request.command.kind());
    r.request_id = request.request_id.clone();
    r.transport = "offline-cache".into();
    r.warnings.push("runtime_unavailable".into());
    let load = (|| {
        request.validate()?;
        if request.refreshes() {
            return Err("invalid_arguments");
        }
        if !path.is_file() {
            return Err("not_initialized");
        }
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| "cache_unavailable")?;
        db.busy_timeout(Duration::from_secs(1))
            .map_err(|_| "cache_unavailable")?;
        db.pragma_update(None, "query_only", true)
            .map_err(|_| "cache_unavailable")?;
        let tx = db.unchecked_transaction().map_err(sql_error)?;
        schema(&tx)?;
        if !access::load(&tx).allows(false, true) {
            return Err("agent_access_denied");
        }
        populate(&tx, request, &mut r)?;
        tx.commit().map_err(sql_error)?;
        Ok(())
    })();
    if let Err(code) = load {
        r.data.accounts.clear();
        errors::fail(&mut r, code);
    }
    r
}
pub fn decimal(value: Option<String>) -> Option<String> {
    value.filter(|s| {
        let raw = s.strip_prefix('-').unwrap_or(s);
        let mut parts = raw.split('.');
        let integer = parts.next().unwrap_or("");
        !integer.is_empty()
            && integer.bytes().all(|b| b.is_ascii_digit())
            && parts
                .next()
                .is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            && parts.next().is_none()
    })
}
pub fn utc(value: Option<String>) -> Option<String> {
    value
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| {
            d.with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
}
pub fn freshness(
    kind: &MetricKind,
    remaining: Option<&str>,
    percent: Option<f64>,
    observed: &str,
    reset: Option<&str>,
    ttl: u64,
    now: DateTime<Utc>,
) -> Freshness {
    if matches!(kind, MetricKind::Quota) && percent.is_none()
        || matches!(kind, MetricKind::Balance) && remaining.is_none()
    {
        return Freshness::Unknown;
    }
    let observed = DateTime::parse_from_rfc3339(observed).ok();
    if observed.is_none_or(|d| {
        now.signed_duration_since(d).num_milliseconds()
            > ttl.min(i64::MAX as u64 / 1000) as i64 * 1000
    }) || reset
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|d| d <= now)
    {
        Freshness::Expired
    } else {
        Freshness::Current
    }
}
pub fn populate(db: &Connection, request: &Request, r: &mut Response) -> Result<(), &'static str> {
    let minutes = db
        .query_row(
            "SELECT value_json FROM app_settings WHERE key='app'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["refreshMinutes"].as_u64())
        .filter(|v| [5, 10, 15, 30].contains(v))
        .unwrap_or(10);
    let ttl = (minutes * 120).max(900);
    let mut stmt=db.prepare("SELECT a.id,a.provider_id,a.alias,a.enabled,COALESCE(f.status,'idle'),f.error_code,f.attempt_at,f.last_success_at,c.next_retry_at FROM accounts a LEFT JOIN fetch_states f ON f.account_id=a.id LEFT JOIN connections c ON c.account_id=a.id ORDER BY a.created_at DESC,a.id").map_err(sql_error)?;
    let rows = stmt
        .query_map([], |row| {
            let status: String = row.get(4)?;
            let code: Option<String> = row.get(5)?;
            let enabled: bool = row.get(3)?;
            let (safe, status, _) = errors::classify(code.as_deref().unwrap_or(&status));
            Ok(Account {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                alias: row.get(2)?,
                enabled,
                status: if !enabled { Status::Paused } else { status },
                error_code: if safe == "ok" || safe == "unknown" {
                    None
                } else {
                    Some(safe.into())
                },
                last_attempt_at: utc(row.get(6)?),
                last_success_at: utc(row.get(7)?),
                next_retry_at: utc(row.get(8)?),
                refreshing: false,
                metrics: vec![],
            })
        })
        .map_err(sql_error)?;
    for row in rows {
        let mut account = row.map_err(sql_error)?;
        if request.account.as_ref().is_some_and(|id| id != &account.id)
            || request
                .provider
                .as_ref()
                .is_some_and(|id| id != &account.provider_id)
            || (request.command == Command::Refresh && request.all && !account.enabled)
        {
            continue;
        }
        if request.command != Command::Accounts {
            account.metrics = metrics(db, &account.id, ttl)?;
            if account.status == Status::Ok && account.metrics.is_empty() {
                account.status = Status::Unknown;
            }
        }
        r.data.accounts.push(account);
    }
    if request.account.is_some() && r.data.accounts.is_empty() {
        return Err("not_found");
    }
    if request.command == Command::Doctor {
        r.data.checks.push(Check {
            name: "schema".into(),
            state: "ok".into(),
            code: None,
        });
        r.data.checks.push(Check {
            name: "agent-access".into(),
            state: "ok".into(),
            code: None,
        });
        r.data.checks.push(Check {
            name: "credentials".into(),
            state: "unchecked".into(),
            code: None,
        });
        r.warnings.push("credentials_unchecked".into());
        for a in &r.data.accounts {
            if let Some(code) = &a.error_code {
                r.data.checks.push(Check {
                    name: format!("account:{}", a.id),
                    state: "error".into(),
                    code: Some(code.clone()),
                });
                r.errors.push(errors::safe_error(code, Some(a.id.clone())));
            }
        }
        if !r.errors.is_empty() {
            r.outcome = "error".into();
        }
    }
    if request.require_fresh
        && r.data.accounts.iter().any(|a| {
            a.metrics.is_empty() || a.metrics.iter().any(|m| m.freshness != Freshness::Current)
        })
    {
        errors::fail(r, "fresh_required");
    }
    Ok(())
}
fn metrics(db: &Connection, id: &str, ttl: u64) -> Result<Vec<Metric>, &'static str> {
    let mut stmt=db.prepare("SELECT metric_id,label,kind,unit,remaining,limit_value,remaining_percent,resets_at,source,observed_at FROM quota_metrics WHERE account_id=?1 ORDER BY metric_id").map_err(sql_error)?;
    let rows = stmt
        .query_map([id], |row| {
            let kind: String = row.get(2)?;
            let kind = if kind == "balance" {
                MetricKind::Balance
            } else {
                MetricKind::Quota
            };
            let remaining = decimal(row.get(4)?);
            let limit = decimal(row.get(5)?);
            let raw: Option<f64> = row.get(6)?;
            let percent = if matches!(kind, MetricKind::Balance) {
                None
            } else {
                raw.filter(|p| p.is_finite() && (0.0..=100.0).contains(p))
            };
            let observed: String = row.get(9)?;
            let resets_at = utc(row.get(7)?);
            let fresh = freshness(
                &kind,
                remaining.as_deref(),
                percent,
                &observed,
                resets_at.as_deref(),
                ttl,
                Utc::now(),
            );
            let source: String = row.get(8)?;
            Ok(Metric {
                metric_id: row.get(0)?,
                label: row.get(1)?,
                remaining_percent: Percent::new(&kind, percent),
                kind,
                unit: row.get(3)?,
                remaining,
                limit,
                resets_at,
                source,
                observed_at: utc(Some(observed)).unwrap_or_default(),
                stale_after_seconds: ttl,
                freshness: fresh,
            })
        })
        .map_err(sql_error)?;
    let metrics = rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)?;
    if metrics.iter().any(|m| {
        m.observed_at.is_empty() || !matches!(m.source.as_str(), "official_api" | "client_endpoint")
    }) {
        return Err("schema_changed");
    }
    Ok(metrics)
}

#[cfg(test)]
mod agent_tests {
    use super::*;
    use crate::request::{Command, Request};
    use std::path::PathBuf;
    struct Fixture {
        db: Connection,
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "stillenough-cli-test-{}.sqlite",
                uuid::Uuid::new_v4()
            ));
            let db = Connection::open(&path).unwrap();
            db.pragma_update(None, "journal_mode", "WAL").unwrap();
            for sql in [
                include_str!("../../../src-tauri/migrations/001_initial.sql"),
                include_str!("../../../src-tauri/migrations/002_quota_only.sql"),
                include_str!("../../../src-tauri/migrations/003_connections.sql"),
                include_str!("../../../src-tauri/migrations/004_cli_authorization.sql"),
                include_str!("../../../src-tauri/migrations/005_monitoring.sql"),
                include_str!("../../../src-tauri/migrations/006_network.sql"),
            ] {
                db.execute_batch(sql).unwrap();
            }
            Self { db, path }
        }
        fn allow(&self) {
            access::save(
                &self.db,
                &access::AgentAccess {
                    version: 1,
                    enabled: true,
                    allow_refresh: true,
                    allow_offline_cache: true,
                    updated_at: Some(Utc::now().to_rfc3339()),
                },
            )
            .unwrap();
        }
        fn seed(&self) {
            let now = Utc::now().to_rfc3339();
            self.db.execute("INSERT INTO accounts VALUES('a','deepseek_api','中文别名', 'DeepSeek',1,?1,?1)",[&now]).unwrap();
            self.db.execute("INSERT INTO connections(account_id,base_url,credential_ref,revision,cli_authorized) VALUES('a','SECRET_HEADER_PATH','SECRET_CREDENTIAL_REF','r',0)",[]).unwrap();
            self.db.execute("INSERT INTO fetch_states(account_id,status,last_success_at) VALUES('a','ok',?1)",[&now]).unwrap();
            self.db.execute("INSERT INTO quota_metrics VALUES('a','wallet','余额','balance','currency_unknown','12345678901234567890.123456',NULL,100,NULL,'official_api',?1)",[&now]).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
            let old = std::mem::replace(&mut self.db, Connection::open_in_memory().unwrap());
            drop(old);
            let _ = std::fs::remove_file(&self.path);
        }
    }
    #[test]
    fn no_database_never_creates_directories() {
        let path =
            std::env::temp_dir().join(format!("absent-{}/data.sqlite", uuid::Uuid::new_v4()));
        let r = offline(&path, &Request::new(Command::Quota, 30));
        assert_eq!(r.errors[0].code, "not_initialized");
        assert!(!path.parent().unwrap().exists());
    }
    #[test]
    fn offline_permission_and_wal_snapshot_are_fail_closed() {
        let f = Fixture::new();
        f.seed();
        let request = Request::new(Command::Quota, 30);
        assert_eq!(
            offline(&f.path, &request).errors[0].code,
            "agent_access_denied"
        );
        f.allow();
        let r = offline(&f.path, &request);
        assert_eq!(r.outcome, "ok");
        assert_eq!(r.data.accounts[0].alias, "中文别名");
        let m = &r.data.accounts[0].metrics[0];
        assert_eq!(m.remaining.as_deref(), Some("12345678901234567890.123456"));
        assert_eq!(
            serde_json::to_value(m).unwrap()["remainingPercent"],
            serde_json::Value::Null
        );
        assert_eq!(m.freshness, Freshness::Current);
        let text = serde_json::to_string(&r).unwrap();
        assert!(!text.contains("SECRET"));
        assert_eq!(r.revision, None);
        assert_eq!(r.instance_id, None);
        assert!(!r.data.accounts[0].refreshing);
        f.db.execute("UPDATE app_settings SET value_json='{\"version\":99,\"enabled\":true}' WHERE key='agent_access'",[]).unwrap();
        assert_eq!(
            offline(&f.path, &request).errors[0].code,
            "agent_access_denied"
        );
    }
    #[test]
    fn schema_rejects_older_newer_and_incomplete_without_migrating() {
        for (sql, code) in [
            (
                "DELETE FROM schema_migrations WHERE version=6",
                "migration_required",
            ),
            (
                "INSERT INTO schema_migrations VALUES(99,'now')",
                "schema_incompatible",
            ),
            (
                "DELETE FROM schema_migrations WHERE version=2",
                "migration_required",
            ),
        ] {
            let f = Fixture::new();
            f.allow();
            f.db.execute_batch(sql).unwrap();
            assert_eq!(
                offline(&f.path, &Request::new(Command::Quota, 30)).errors[0].code,
                code
            );
        }
    }
    #[test]
    fn strict_fresh_keeps_old_values_and_accounts_list_omits_metrics() {
        let f = Fixture::new();
        f.allow();
        f.seed();
        f.db.execute(
            "UPDATE quota_metrics SET observed_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
        let mut request = Request::new(Command::Quota, 30);
        request.require_fresh = true;
        let r = offline(&f.path, &request);
        assert_eq!(crate::errors::exit_code(&r), 11);
        assert_eq!(r.data.accounts[0].metrics[0].freshness, Freshness::Expired);
        assert!(r.data.accounts[0].metrics[0].remaining.is_some());
        let r = offline(&f.path, &Request::new(Command::Accounts, 30));
        assert!(r.data.accounts[0].metrics.is_empty());
        assert_eq!(r.outcome, "ok");
    }
    #[test]
    fn status_and_freshness_are_independent() {
        let f = Fixture::new();
        f.allow();
        f.seed();
        f.db.execute(
            "UPDATE fetch_states SET status='auth_required',error_code='auth_required'",
            [],
        )
        .unwrap();
        let r = offline(&f.path, &Request::new(Command::Quota, 30));
        assert_eq!(r.outcome, "ok");
        assert_eq!(r.data.accounts[0].status, Status::AuthRequired);
        assert_eq!(r.data.accounts[0].metrics[0].freshness, Freshness::Current);
        let mut request = Request::new(Command::Doctor, 30);
        request.offline = true;
        assert_eq!(crate::errors::exit_code(&offline(&f.path, &request)), 4);
    }
    #[test]
    fn freshness_boundaries_zero_missing_and_reset() {
        let now = DateTime::parse_from_rfc3339("2026-10-18T00:20:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let observed = "2026-10-18T00:00:00Z";
        assert_eq!(
            freshness(
                &MetricKind::Quota,
                None,
                Some(0.0),
                observed,
                None,
                1200,
                now
            ),
            Freshness::Current
        );
        assert_eq!(
            freshness(&MetricKind::Quota, None, None, observed, None, 1200, now),
            Freshness::Unknown
        );
        assert_eq!(
            freshness(
                &MetricKind::Balance,
                Some("0"),
                None,
                observed,
                Some("2026-10-18T00:20:00Z"),
                1200,
                now
            ),
            Freshness::Expired
        );
        assert_eq!(
            freshness(
                &MetricKind::Quota,
                None,
                Some(25.0),
                "invalid",
                None,
                1200,
                now
            ),
            Freshness::Expired
        );
        assert_eq!(decimal(Some("1e5".into())), None);
        assert_eq!(decimal(Some("0".into())), Some("0".into()));
    }
}

#[cfg(test)]
mod shared_freshness_tests {
    use super::*;
    #[test]
    fn desktop_cli_fixture_is_identical() {
        let raw =
            include_str!("../../../fixtures/cli/freshness.json").trim_start_matches('\u{feff}');
        let cases: serde_json::Value = serde_json::from_str(raw).unwrap();
        for c in cases.as_array().unwrap() {
            let kind = if c["kind"] == "balance" {
                MetricKind::Balance
            } else {
                MetricKind::Quota
            };
            let now = DateTime::parse_from_rfc3339(c["now"].as_str().unwrap())
                .unwrap()
                .with_timezone(&Utc);
            let result = freshness(
                &kind,
                c["remaining"].as_str(),
                c["percent"].as_f64(),
                c["observed"].as_str().unwrap(),
                c["reset"].as_str(),
                c["ttl"].as_u64().unwrap(),
                now,
            );
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                c["expected"],
                "{}",
                c["name"]
            );
        }
    }
}

#[cfg(test)]
mod schema_examples {
    use super::*;
    #[test]
    fn safe_snapshot_contract_examples() {
        let db = Connection::open_in_memory().unwrap();
        for sql in [
            include_str!("../../../src-tauri/migrations/001_initial.sql"),
            include_str!("../../../src-tauri/migrations/002_quota_only.sql"),
            include_str!("../../../src-tauri/migrations/003_connections.sql"),
            include_str!("../../../src-tauri/migrations/004_cli_authorization.sql"),
            include_str!("../../../src-tauri/migrations/005_monitoring.sql"),
            include_str!("../../../src-tauri/migrations/006_network.sql"),
        ] {
            db.execute_batch(sql).unwrap();
        }
        db.execute_batch("INSERT INTO accounts VALUES('sample','deepseek_api','测试账号','DeepSeek',1,'now','now');INSERT INTO fetch_states(account_id,status) VALUES('sample','auth_required');INSERT INTO quota_metrics VALUES('sample','wallet','余额','balance','currency_unknown','0',NULL,100,NULL,'official_api','2026-10-17T00:00:00Z');").unwrap();
        let target =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/cli-schema-cases");
        std::fs::create_dir_all(&target).unwrap();
        for command in [Command::Accounts, Command::Quota, Command::Doctor] {
            let mut request = Request::new(command, 30);
            request.require_fresh = request.command == Command::Quota;
            let mut r = Response::local(request.command.name(), request.command.kind());
            populate(&db, &request, &mut r).unwrap();
            std::fs::write(
                target.join(format!("{}.json", request.command.kind())),
                serde_json::to_vec(&r).unwrap(),
            )
            .unwrap();
        }
    }
}
