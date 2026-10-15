use crate::domain::models::{Account, QuotaMetric};
use rusqlite::Connection;

pub fn list(connection: &Connection) -> Result<Vec<Account>, String> {
    let mut statement = connection.prepare(
        "SELECT a.id,a.provider_id,a.alias,a.product_name,a.enabled,COALESCE(f.status,'idle'),f.last_success_at,c.base_url,COALESCE(c.cli_authorized,0),f.attempt_at,c.next_retry_at,c.network_json
         FROM accounts a LEFT JOIN fetch_states f ON f.account_id=a.id LEFT JOIN connections c ON c.account_id=a.id ORDER BY a.created_at DESC"
    ).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(Account {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                alias: row.get(2)?,
                product_name: row.get(3)?,
                enabled: row.get(4)?,
                metrics: vec![],
                fetch_status: row.get(5)?,
                last_success_at: row.get(6)?,
                base_url: row.get(7)?,
                cli_authorized: row.get(8)?,
                last_attempt_at: row.get(9)?,
                next_retry_at: row.get(10)?,
                refreshing: false,
                network: row
                    .get::<_, Option<String>>(11)?
                    .map(|json| {
                        serde_json::from_str(&json).unwrap_or(super::network::NetworkConfig {
                            mode: "invalid".into(),
                            proxy_url: String::new(),
                        })
                    })
                    .unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?;
    let mut accounts = Vec::new();
    for row in rows {
        let mut account = row.map_err(|e| e.to_string())?;
        account.metrics = list_metrics(connection, &account.id)?;
        accounts.push(account);
    }
    Ok(accounts)
}

fn list_metrics(connection: &Connection, account_id: &str) -> Result<Vec<QuotaMetric>, String> {
    let mut statement = connection.prepare(
        "SELECT metric_id,label,kind,unit,remaining,limit_value,remaining_percent,resets_at,source,observed_at \
         FROM quota_metrics WHERE account_id=?1 ORDER BY metric_id",
    ).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([account_id], |row| {
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
                stale_after_seconds: 1200,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.map(|row| row.map_err(|e| e.to_string())).collect()
}

pub fn get(connection: &Connection, id: &str) -> Result<Option<Account>, String> {
    let accounts = list(connection)?;
    Ok(accounts.into_iter().find(|account| account.id == id))
}

pub fn remove(connection: &Connection, id: &str) -> Result<bool, String> {
    connection
        .execute("DELETE FROM accounts WHERE id=?1", [id])
        .map(|count| count > 0)
        .map_err(|e| e.to_string())
}
