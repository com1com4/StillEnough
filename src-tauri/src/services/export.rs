use crate::domain::models::{ExportSummary, QuotaMetric};
use std::path::Path;

/// Whitelisted export shape. Credentials, credential references, base URLs, connection
/// revisions and local paths never reach the file.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportAccount {
    alias: String,
    provider_id: String,
    product_name: String,
    enabled: bool,
    fetch_status: String,
    last_success_at: Option<String>,
    last_attempt_at: Option<String>,
    metrics: Vec<QuotaMetric>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportDocument {
    application: &'static str,
    schema_version: u32,
    exported_at: String,
    note: &'static str,
    accounts: Vec<ExportAccount>,
}

fn load(db: &rusqlite::Connection) -> Result<Vec<ExportAccount>, String> {
    let mut statement = db
        .prepare(
            "SELECT a.id,a.alias,a.provider_id,a.product_name,a.enabled,COALESCE(f.status,'idle'),f.last_success_at,f.attempt_at \
             FROM accounts a LEFT JOIN fetch_states f ON f.account_id=a.id ORDER BY a.created_at DESC",
        )
        .map_err(|error| format!("导出读取失败：{error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ExportAccount {
                    alias: row.get(1)?,
                    provider_id: row.get(2)?,
                    product_name: row.get(3)?,
                    enabled: row.get(4)?,
                    fetch_status: row.get(5)?,
                    last_success_at: row.get(6)?,
                    last_attempt_at: row.get(7)?,
                    metrics: Vec::new(),
                },
            ))
        })
        .map_err(|error| format!("导出读取失败：{error}"))?;
    let mut accounts = Vec::new();
    for row in rows {
        let (id, mut account) = row.map_err(|error| format!("导出读取失败：{error}"))?;
        let mut metrics = db
            .prepare(
                "SELECT metric_id,label,kind,unit,remaining,limit_value,remaining_percent,resets_at,source,observed_at \
                 FROM quota_metrics WHERE account_id=?1 ORDER BY metric_id",
            )
            .map_err(|error| format!("导出读取失败：{error}"))?;
        account.metrics = metrics
            .query_map([&id], |row| {
                Ok(QuotaMetric {
                    metric_id: row.get(0)?,
                    label: row.get(1)?,
                    kind: row.get(2)?,
                    unit: row.get(3)?,
                    remaining: row.get(4)?,
                    limit: row.get(5)?,
                    remaining_percent: row.get(6)?,
                    resets_at: row.get(7)?,
                    source: row.get(8)?,
                    observed_at: row.get(9)?,
                    stale_after_seconds: 0,
                })
            })
            .map_err(|error| format!("导出读取失败：{error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("导出读取失败：{error}"))?;
        accounts.push(account);
    }
    Ok(accounts)
}

fn cell(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub fn to_csv(db: &rusqlite::Connection) -> Result<String, String> {
    let mut out = String::from(
        "alias,providerId,productName,enabled,fetchStatus,lastSuccessAt,lastAttemptAt,metricId,metricLabel,kind,unit,remaining,limit,remainingPercent,resetsAt,source,observedAt\n",
    );
    for account in load(db)? {
        let head = [
            csv_field(&account.alias),
            csv_field(&account.provider_id),
            csv_field(&account.product_name),
            account.enabled.to_string(),
            csv_field(&account.fetch_status),
            csv_field(&cell(&account.last_success_at)),
            csv_field(&cell(&account.last_attempt_at)),
        ]
        .join(",");
        if account.metrics.is_empty() {
            out.push_str(&format!("{head},,,,,,,,,\n"));
            continue;
        }
        for metric in &account.metrics {
            let percent = metric
                .remaining_percent
                .map(|value| format!("{value:.4}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "{head},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&metric.metric_id),
                csv_field(&metric.label),
                csv_field(&metric.kind),
                csv_field(&metric.unit),
                csv_field(&cell(&metric.remaining)),
                csv_field(&cell(&metric.limit)),
                percent,
                csv_field(&cell(&metric.resets_at)),
                csv_field(&metric.source),
                csv_field(&metric.observed_at),
            ));
        }
    }
    Ok(out)
}

pub struct Rendered {
    pub content: String,
    pub account_count: usize,
    pub row_count: usize,
}

pub fn render(db: &rusqlite::Connection, format: &str) -> Result<Rendered, String> {
    let accounts = load(db)?;
    let account_count = accounts.len();
    let row_count: usize = accounts
        .iter()
        .map(|account| account.metrics.len().max(1))
        .sum();
    let content = match format {
        "json" => {
            let document = ExportDocument {
                application: "StillEnough",
                schema_version: 1,
                exported_at: chrono::Utc::now().to_rfc3339(),
                note:
                    "导出仅包含本工具保存的账号与观测数据，不含凭据、凭据引用、连接地址或本地路径。",
                accounts,
            };
            serde_json::to_string_pretty(&document)
                .map_err(|error| format!("导出序列化失败：{error}"))?
        }
        "csv" => to_csv(db)?,
        _ => return Err("导出格式仅支持 JSON 或 CSV".into()),
    };
    Ok(Rendered {
        content,
        account_count,
        row_count,
    })
}

pub fn write_file(
    directory: &Path,
    format: &str,
    rendered: &Rendered,
) -> Result<ExportSummary, String> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let file_name = format!("stillenough-export-{stamp}.{format}");
    let path = directory.join(&file_name);
    std::fs::write(&path, &rendered.content)
        .map_err(|error| format!("导出文件写入失败：{error}"))?;
    Ok(ExportSummary {
        format: format.to_string(),
        file_name,
        path: path.to_string_lossy().to_string(),
        account_count: rendered.account_count,
        row_count: rendered.row_count,
    })
}

/// Delete every account, observation, alert and connection this tool owns. Credential
/// references are queued for cleanup before the rows that referenced them disappear.
pub fn clear_all(db: &mut rusqlite::Connection) -> Result<usize, String> {
    let tx = db
        .transaction()
        .map_err(|error| format!("清库事务失败：{error}"))?;
    tx.execute(
        "INSERT OR IGNORE INTO credential_cleanup(credential_ref) \
         SELECT credential_ref FROM connections WHERE credential_ref<>''",
        [],
    )
    .map_err(|error| format!("凭据清理登记失败：{error}"))?;
    tx.execute("DELETE FROM app_settings WHERE key='agent_access'", [])
        .map_err(|_| "撤销 Agent 权限失败")?;
    let removed = tx
        .execute("DELETE FROM accounts", [])
        .map_err(|error| format!("清库失败：{error}"))?;
    tx.commit().map_err(|error| format!("清库失败：{error}"))?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        db.execute(
            "INSERT INTO connections(account_id,base_url,credential_ref,revision) VALUES('a','https://api.deepseek.com','secret-ref','rev-1')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO fetch_states(account_id,status,last_success_at) VALUES('a','ok','2026-10-01T00:00:00Z')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO quota_metrics(account_id,metric_id,label,kind,unit,remaining,remaining_percent,source,observed_at) \
             VALUES('a','balance_USD','可用余额 · USD','balance','USD','12.50 USD',NULL,'official_api','2026-10-01T00:00:00Z'),\
             ('a','quota_5h','5 小时窗口, 含逗号','quota','%','40.0%',40.0,'official_api','2026-10-01T00:00:00Z')",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn exports_contain_observations_but_never_credentials_or_paths() {
        let db = database();
        let json = render(&db, "json").unwrap();
        let csv = render(&db, "csv").unwrap();
        assert_eq!(json.account_count, 1);
        assert_eq!(csv.row_count, 2);
        for text in [&json.content, &csv.content] {
            for forbidden in [
                "secret-ref",
                "rev-1",
                "https://api.deepseek.com",
                "redential",
            ] {
                assert!(!text.contains(forbidden), "导出泄露了 {forbidden}");
            }
        }
        assert!(json.content.contains("\"productName\": \"DeepSeek API\""));
        assert!(csv.content.lines().count() == 3, "表头加两行指标");
        assert!(
            csv.content.contains("\"5 小时窗口, 含逗号\""),
            "CSV 需要转义逗号"
        );
        assert!(csv.content.contains("40.0000"));
        assert!(render(&db, "xml").is_err());
    }

    #[test]
    fn clearing_all_data_queues_credentials_and_leaves_no_rows() {
        let mut db = database();
        db.execute(
            "INSERT INTO observations(account_id,observed_at,metrics_json) VALUES('a','now','[]')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO alert_events(account_id,metric_id,metric_label,kind,unit,threshold,crossed_thresholds,remaining_percent,period_key,dedupe_key,triggered_at) \
             VALUES('a','quota_5h','5 小时窗口','quota','%',5.0,'[5.0]',5.0,'rolling','k','now')",
            [],
        )
        .unwrap();
        assert_eq!(clear_all(&mut db).unwrap(), 1);
        for table in [
            "accounts",
            "quota_metrics",
            "fetch_states",
            "connections",
            "observations",
            "alert_events",
            "alert_state",
        ] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0,
                "{table} 未清空"
            );
        }
        assert_eq!(
            db.query_row("SELECT credential_ref FROM credential_cleanup", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "secret-ref"
        );
    }

    #[test]
    fn export_file_uses_the_requested_extension_and_content() {
        let directory = std::env::temp_dir().join(format!("apm-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let rendered = Rendered {
            content: "a,b\n1,2\n".into(),
            account_count: 1,
            row_count: 2,
        };
        let summary = write_file(&directory, "csv", &rendered).unwrap();
        assert!(summary.file_name.ends_with(".csv"));
        assert_eq!(
            std::fs::read_to_string(&summary.path).unwrap(),
            "a,b\n1,2\n"
        );
        assert_eq!(summary.row_count, 2);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
