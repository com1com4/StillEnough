use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAccess {
    pub version: u8,
    pub enabled: bool,
    pub allow_refresh: bool,
    pub allow_offline_cache: bool,
    pub updated_at: Option<String>,
}
impl Default for AgentAccess {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            allow_refresh: false,
            allow_offline_cache: false,
            updated_at: None,
        }
    }
}
impl AgentAccess {
    pub fn allows(&self, refresh: bool, offline: bool) -> bool {
        self.version == 1
            && self
                .updated_at
                .as_deref()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .is_some()
            && self.enabled
            && (!refresh || self.allow_refresh)
            && (!offline || self.allow_offline_cache)
    }
}
pub fn load(db: &Connection) -> AgentAccess {
    db.query_row(
        "SELECT value_json FROM app_settings WHERE key='agent_access'",
        [],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|v| serde_json::from_str::<AgentAccess>(&v).ok())
    .filter(|v| v.version == 1)
    .unwrap_or_default()
}
pub fn save(db: &Connection, value: &AgentAccess) -> Result<(), &'static str> {
    if value.version != 1 {
        return Err("invalid_arguments");
    }
    let tx = db.unchecked_transaction().map_err(|_| "database_error")?;
    tx.execute("INSERT INTO app_settings(key,value_json,updated_at) VALUES('agent_access',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",rusqlite::params![serde_json::to_string(value).map_err(|_|"internal_error")?,chrono::Utc::now().to_rfc3339()]).map_err(|_|"database_error")?;
    tx.commit().map_err(|_| "database_error")
}
