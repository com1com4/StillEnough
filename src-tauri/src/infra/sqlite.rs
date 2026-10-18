use rusqlite::Connection;
use std::{fs, path::Path};

pub fn open_database(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建本地数据目录：{error}"))?;
    }
    let connection =
        Connection::open(path).map_err(|error| format!("无法打开本地数据库：{error}"))?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| error.to_string())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "database_busy")?;
    reject_future_schema(&connection)?;
    let version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if version < 6 && path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        let backup = path.with_extension(format!("pre-cli-{}.sqlite", uuid::Uuid::new_v4()));
        connection
            .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
            .map_err(|_| "数据库一致性备份失败，未执行迁移")?;
    }
    let mode: String = connection
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
        .map_err(|_| "数据库 WAL 初始化失败")?;
    if mode != "wal" {
        return Err("数据库不支持 WAL，停止启动".into());
    }
    let health: String = connection
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|_| "数据库完整性检查失败")?;
    if health != "ok" {
        return Err("数据库完整性检查失败".into());
    }
    migrate(&connection)?;
    Ok(connection)
}

fn reject_future_schema(connection: &Connection) -> Result<(), String> {
    let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='schema_migrations' AND type='table')", [], |r| r.get(0)).map_err(|_| "数据库版本读取失败")?;
    if exists {
        let version: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "数据库版本读取失败")?;
        if version > 6 {
            return Err("schema_incompatible".into());
        }
    }
    Ok(())
}

fn migrate(connection: &Connection) -> Result<(), String> {
    reject_future_schema(connection)?;
    let has_migrations: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    if !has_migrations {
        connection
            .execute_batch(include_str!("../../migrations/001_initial.sql"))
            .map_err(|error| format!("数据库迁移失败：{error}"))?;
    }
    let has_quota_only: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=2)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !has_quota_only {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute_batch(include_str!("../../migrations/002_quota_only.sql"))
            .map_err(|error| format!("数据库迁移失败：{error}"))?;
        transaction.commit().map_err(|error| error.to_string())?;
    }
    let has_connections: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=3)",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !has_connections {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        transaction
            .execute_batch(include_str!("../../migrations/003_connections.sql"))
            .map_err(|e| format!("数据库迁移失败：{e}"))?;
        transaction.commit().map_err(|e| e.to_string())?;
    }
    let has_cli: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=4)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !has_cli {
        let tx = connection
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute_batch(include_str!("../../migrations/004_cli_authorization.sql"))
            .map_err(|e| format!("数据库迁移失败：{e}"))?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let has_monitoring: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=5)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !has_monitoring {
        let tx = connection
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute_batch(include_str!("../../migrations/005_monitoring.sql"))
            .map_err(|e| format!("数据库迁移失败：{e}"))?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=6)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        let tx = connection
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute_batch(include_str!("../../migrations/006_network.sql"))
            .map_err(|e| format!("数据库迁移失败：{e}"))?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::migrate;
    use rusqlite::Connection;
    #[test]
    fn network_upgrade_preserves_connections_and_accounts_without_connections() {
        let db = Connection::open_in_memory().unwrap();
        for sql in [
            include_str!("../../migrations/001_initial.sql"),
            include_str!("../../migrations/002_quota_only.sql"),
            include_str!("../../migrations/003_connections.sql"),
            include_str!("../../migrations/004_cli_authorization.sql"),
            include_str!("../../migrations/005_monitoring.sql"),
        ] {
            db.execute_batch(sql).unwrap();
        }
        db.execute_batch("INSERT INTO accounts VALUES('a','deepseek_api','a','DeepSeek API',1,'now','now');
            INSERT INTO accounts VALUES('unknown','other','unknown','unknown',0,'now','now');
            INSERT INTO connections(account_id,base_url,credential_ref,revision) VALUES('a','https://api.deepseek.com','opaque-ref','revision');
            INSERT INTO app_settings VALUES('app','{\"refreshMinutes\":15,\"widgetVisible\":false,\"theme\":\"dark\"}','now');").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let accounts = crate::services::accounts::list(&db).unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(accounts.iter().all(|a| a.network.mode == "inherit"));
        assert_eq!(
            db.query_row("SELECT credential_ref FROM connections", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "opaque-ref"
        );
        assert_eq!(crate::commands::load_settings(&db).refresh_minutes, 15);
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version=6",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn cli_upgrade_pauses_old_token_connection_and_preserves_history() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/002_quota_only.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/003_connections.sql"))
            .unwrap();
        db.execute_batch("INSERT INTO accounts VALUES('cli','bailian_token_plan','a','p',1,'now','now');
            INSERT INTO connections(account_id,base_url,credential_ref,revision) VALUES('cli','https://bailian-cs.console.aliyun.com','opaque-old-ref','old');
            INSERT INTO observations(account_id,observed_at,metrics_json) VALUES('cli','now','[]');
            INSERT INTO fetch_states(account_id,status,last_success_at) VALUES('cli','ok','now');").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        assert!(!db
            .query_row("SELECT enabled FROM accounts", [], |r| r.get::<_, bool>(0))
            .unwrap());
        assert!(!db
            .query_row("SELECT cli_authorized FROM connections", [], |r| r
                .get::<_, bool>(0))
            .unwrap());
        assert_eq!(
            db.query_row("SELECT credential_ref FROM credential_cleanup", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "opaque-old-ref"
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM observations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.execute("UPDATE connections SET cli_authorized=1", [])
            .unwrap();
        migrate(&db).unwrap();
        assert!(db
            .query_row("SELECT cli_authorized FROM connections", [], |r| r
                .get::<_, bool>(0))
            .unwrap());
    }
    #[test]
    fn deletes_manual_preserves_demo_and_unknown_accounts_and_is_repeatable() {
        let db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "foreign_keys", "ON").unwrap();
        db.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        db.execute_batch("INSERT INTO accounts VALUES('manual','manual','a','p',1,'now','now');
   INSERT INTO accounts VALUES('demo','mock','a','p',1,'now','now');
   INSERT INTO accounts VALUES('unknown','other','a','p',1,'now','now');
   INSERT INTO accounts VALUES('mixed','manual','a','p',1,'now','now');
   INSERT INTO quota_metrics(account_id,metric_id,label,kind,unit,source,observed_at)
    VALUES('manual','m','m','balance','CNY','manual','now'),('demo','d','d','balance','CNY','demo','now'),
    ('mixed','old','old','balance','CNY','manual','now'),('mixed','keep','keep','balance','CNY','demo','now');
   INSERT INTO fetch_states(account_id,status) VALUES('manual','ok');").unwrap();
        db.pragma_update(None, "ignore_check_constraints", "ON")
            .unwrap();
        db.execute("INSERT INTO quota_metrics(account_id,metric_id,label,kind,unit,source,observed_at) VALUES('mixed','real','real','balance','USD','official_api','now')",[]).unwrap();
        db.pragma_update(None, "ignore_check_constraints", "OFF")
            .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM accounts WHERE id='manual'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM quota_metrics", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM fetch_states", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(db.prepare("SELECT * FROM subscriptions").is_err());
    }

    #[test]
    fn monitoring_upgrade_creates_alert_tables_and_is_idempotent() {
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
        db.execute_batch(
            "INSERT INTO accounts VALUES('a','deepseek_api','a','DeepSeek API',1,'now','now');
            INSERT INTO observations(account_id,observed_at,metrics_json) VALUES('a','now','[]');",
        )
        .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        for table in ["alert_state", "alert_events"] {
            assert!(db.prepare(&format!("SELECT * FROM {table}")).is_ok());
        }
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM observations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1,
            "升级不得删除已有观测"
        );
        // The unique dedupe key is what keeps a restart from replaying an alert.
        db.execute(
            "INSERT INTO alert_events(account_id,metric_id,metric_label,kind,unit,threshold,crossed_thresholds,remaining_percent,period_key,dedupe_key,triggered_at) \
             VALUES('a','m','m','quota','%',5.0,'[5.0]',5.0,'rolling','k','now')",
            [],
        )
        .unwrap();
        assert!(db
            .execute(
                "INSERT INTO alert_events(account_id,metric_id,metric_label,kind,unit,threshold,crossed_thresholds,remaining_percent,period_key,dedupe_key,triggered_at) \
                 VALUES('a','m','m','quota','%',5.0,'[5.0]',5.0,'rolling','k','now')",
                [],
            )
            .is_err());
    }
}

#[cfg(test)]
mod cli_database_tests {
    use super::*;
    #[test]
    fn future_schema_is_rejected_before_migration() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT);INSERT INTO schema_migrations VALUES(99,'now');").unwrap();
        assert_eq!(migrate(&db).unwrap_err(), "schema_incompatible");
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn wal_writer_backs_up_legacy_database_consistently() {
        let dir =
            std::env::temp_dir().join(format!("stillenough-wal-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.sqlite");
        let old = Connection::open(&path).unwrap();
        old.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        drop(old);
        let db = open_database(&path).unwrap();
        assert_eq!(
            db.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert_eq!(
            db.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            6
        );
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("pre-cli-"))
            .collect();
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(backups[0].path()).unwrap();
        assert_eq!(
            backup
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(backup);
        drop(db);
        for file in std::fs::read_dir(&dir).unwrap() {
            std::fs::remove_file(file.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
