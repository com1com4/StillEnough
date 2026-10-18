use crate::{
    commands::AppState,
    domain::{
        models::{AccountInput, MonitoringSettings, ProbeResult, QuotaMetric},
        validation::validate_account,
    },
    infra::credentials,
    providers::{registry::get_provider, FetchError},
    services::{accounts, alerts, history},
};
use rusqlite::params;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use uuid::Uuid;
use zeroize::Zeroizing;

pub struct Setup {
    network_revision: u64,
    pub input: AccountInput,
    pub key: Option<Zeroizing<String>>,
    pub old_ref: Option<String>,
    pub old_revision: Option<String>,
    pub expires: Instant,
    pub probe: Option<(Instant, Vec<QuotaMetric>)>,
    pub busy: bool,
    pub next_attempt: Instant,
}
pub struct Gate {
    pub busy: bool,
    pub next_attempt: Instant,
    pub failures: usize,
}
#[cfg(test)]
type TestFetch = Arc<dyn Fn() -> crate::providers::FetchFuture<'static> + Send + Sync>;
pub struct Runtime {
    #[cfg(test)]
    pub test_fetch: Mutex<Option<TestFetch>>,
    pub network: super::network::Network,
    pub sessions: Mutex<HashMap<String, Setup>>,
    pub gates: Mutex<HashMap<String, Gate>>,
    pub provider_cooldown: Mutex<HashMap<String, Instant>>,
    pub concurrency: Arc<tokio::sync::Semaphore>,
}
impl Runtime {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            #[cfg(test)]
            test_fetch: Mutex::new(None),
            network: super::network::Network::new(),
            sessions: Mutex::new(HashMap::new()),
            gates: Mutex::new(HashMap::new()),
            provider_cooldown: Mutex::new(HashMap::new()),
            concurrency: Arc::new(tokio::sync::Semaphore::new(3)),
        })
    }
    pub fn prune(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.retain(|_, s| s.expires > Instant::now());
        }
    }
}
fn db_lock(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, crate::commands::Databases>, String> {
    state.databases.lock().map_err(|_| "本地状态不可用".into())
}
pub fn begin(state: &AppState, input: AccountInput) -> Result<String, String> {
    validate_account(&input)?;
    input.network.validate(false)?;
    state.runtime.prune();
    let existing = if let Some(id) = &input.id {
        let db = db_lock(state)?;
        let account = accounts::get(&db.production, id)?.ok_or("账号不存在")?;
        if account.provider_id != input.provider_id {
            return Err("已有账号不能切换产品，请添加新账号".into());
        }
        Some(
            db.production
                .query_row(
                    "SELECT credential_ref,revision FROM connections WHERE account_id=?1",
                    [id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .map_err(|_| "连接不存在")?,
        )
    } else {
        None
    };
    let id = Uuid::new_v4().to_string();
    let (old_ref, old_revision) = existing
        .map(|(a, b)| (Some(a), Some(b)))
        .unwrap_or((None, None));
    let mut sessions = state
        .runtime
        .sessions
        .lock()
        .map_err(|_| "配置会话不可用")?;
    if sessions.len() >= 8 {
        return Err("配置会话过多，请关闭后重试".into());
    }
    sessions.insert(
        id.clone(),
        Setup {
            network_revision: state.runtime.network.revision(),
            input,
            key: None,
            old_ref,
            old_revision,
            expires: Instant::now() + Duration::from_secs(600),
            probe: None,
            busy: false,
            next_attempt: Instant::now(),
        },
    );
    Ok(id)
}
pub fn set_key(state: &AppState, id: &str, key: Zeroizing<String>) -> Result<(), String> {
    if key.trim().is_empty() || key.len() > 4096 || key.chars().any(|c| c.is_control()) {
        return Err("请填写有效的 API Key".into());
    }
    let mut sessions = state
        .runtime
        .sessions
        .lock()
        .map_err(|_| "配置会话不可用")?;
    let s = sessions
        .get_mut(id)
        .filter(|s| s.expires > Instant::now())
        .ok_or("配置已过期，请重试")?;
    if get_provider(&s.input.provider_id).is_some_and(|p| !p.requires_credential()) {
        return Err("CLI 连接不接受 API Key 或临时令牌".into());
    }
    if s.busy {
        return Err("读取中，请稍后替换 Key".into());
    }
    s.key = Some(key);
    s.probe = None;
    Ok(())
}
pub fn cancel(state: &AppState, id: &str) {
    if let Ok(mut sessions) = state.runtime.sessions.lock() {
        sessions.remove(id);
    }
}
fn remaining_wait(instant: Instant) -> bool {
    instant > Instant::now()
}
async fn fetch(
    runtime: &Runtime,
    input: &AccountInput,
    key: &str,
) -> Result<Vec<QuotaMetric>, FetchError> {
    fetch_guarded(runtime, input, key, None).await
}

type AccessGuard<'a> = &'a (dyn Fn() -> Result<(), FetchError> + Send + Sync);
async fn fetch_guarded(
    runtime: &Runtime,
    input: &AccountInput,
    key: &str,
    guard: Option<AccessGuard<'_>>,
) -> Result<Vec<QuotaMetric>, FetchError> {
    if let Some(check) = guard {
        check()?;
    }
    let scope = format!("{}:{}", input.provider_id, input.base_url);
    let cooldown = runtime
        .provider_cooldown
        .lock()
        .map_err(|_| FetchError::new("network_error"))?
        .get(&scope)
        .copied();
    if let Some(until) = cooldown.filter(|until| remaining_wait(*until)) {
        let mut error = FetchError::new("rate_limited");
        error.retry_after_seconds = until
            .saturating_duration_since(Instant::now())
            .as_secs()
            .max(1);
        return Err(error);
    }
    let _permit = runtime
        .concurrency
        .acquire()
        .await
        .map_err(|_| FetchError::new("network_error"))?;
    // 等待 semaphore 后必须再检查撤销和同平台冷却。
    if let Some(check) = guard {
        check()?;
    }
    if let Some(until) = runtime
        .provider_cooldown
        .lock()
        .map_err(|_| FetchError::new("network_error"))?
        .get(&scope)
        .copied()
        .filter(|until| remaining_wait(*until))
    {
        let mut error = FetchError::new("rate_limited");
        error.retry_after_seconds = until
            .saturating_duration_since(Instant::now())
            .as_secs()
            .max(1);
        return Err(error);
    }
    let provider =
        get_provider(&input.provider_id).ok_or_else(|| FetchError::new("unsupported"))?;
    if !provider.requires_credential() && !input.cli_authorized {
        return Err(FetchError::new("cli_authorization_required"));
    }
    #[cfg(test)]
    {
        let fake = runtime.test_fetch.lock().unwrap().clone();
        if let Some(fake) = fake {
            let result = fake().await;
            if let Err(e) = &result {
                if e.code == "rate_limited" {
                    runtime.provider_cooldown.lock().unwrap().insert(
                        scope,
                        Instant::now() + Duration::from_secs(e.retry_after_seconds),
                    );
                }
            }
            return result;
        }
    }
    let config = runtime
        .network
        .resolve(&input.network)
        .map_err(|_| FetchError::new("proxy_config_invalid"))?;
    if provider.network_kind() == "cli_managed" && config.mode != "system" {
        return Err(FetchError::new("cli_proxy_unsupported"));
    }
    let client_config = if provider.network_kind() == "http_client" {
        config.clone()
    } else {
        super::network::NetworkConfig {
            mode: "direct".into(),
            proxy_url: String::new(),
        }
    };
    let client = runtime
        .network
        .client(&client_config)
        .map_err(|_| FetchError::new("proxy_config_invalid"))?;
    let result = provider
        .fetch_routed(&client, &input.base_url, key, &config)
        .await;
    if let Err(error) = &result {
        if error.code == "rate_limited" {
            if let Ok(mut cooldown) = runtime.provider_cooldown.lock() {
                cooldown.insert(
                    scope,
                    Instant::now() + Duration::from_secs(error.retry_after_seconds),
                );
            }
        }
    }
    result
}
pub async fn probe(app: tauri::AppHandle, id: String) -> Result<ProbeResult, String> {
    let state = app.state::<AppState>();
    let (input, key, old_ref) = {
        let mut sessions = state
            .runtime
            .sessions
            .lock()
            .map_err(|_| "配置会话不可用")?;
        let s = sessions
            .get_mut(&id)
            .filter(|s| s.expires > Instant::now())
            .ok_or("配置已过期，请重试")?;
        if s.busy || remaining_wait(s.next_attempt) {
            return Err("请等待至少 30 秒后重试读取".into());
        }
        s.busy = true;
        s.probe = None;
        s.next_attempt = Instant::now() + Duration::from_secs(30);
        (
            s.input.clone(),
            s.key.as_ref().map(|k| Zeroizing::new(k.to_string())),
            s.old_ref.clone(),
        )
    };
    let is_cli = get_provider(&input.provider_id).is_some_and(|p| !p.requires_credential());
    let key_result = if is_cli {
        Ok(Zeroizing::new(String::new()))
    } else if let Some(key) = key {
        Ok(key)
    } else if let Some(reference) = old_ref {
        tauri::async_runtime::spawn_blocking(move || credentials::read(&reference))
            .await
            .map_err(|_| "凭据读取失败".to_string())
            .and_then(|r| r)
    } else {
        Err("请配置 API Key".into())
    };
    let result = match key_result {
        Ok(key) => fetch(&state.runtime, &input, &key).await,
        Err(_) => Err(FetchError::new("auth_required")),
    };
    let mut sessions = state
        .runtime
        .sessions
        .lock()
        .map_err(|_| "配置会话不可用")?;
    let s = sessions
        .get_mut(&id)
        .filter(|s| s.expires > Instant::now())
        .ok_or("配置已取消或过期")?;
    s.busy = false;
    match result {
        Ok(metrics) => {
            s.probe = Some((Instant::now(), metrics.clone()));
            Ok(ProbeResult {
                observed_at: metrics[0].observed_at.clone(),
                metrics,
                identity_status: if is_cli {
                    "cli_current_login_unverified"
                } else {
                    "key_scope_unverified"
                },
            })
        }
        Err(e) => {
            s.next_attempt = Instant::now() + Duration::from_secs(e.retry_after_seconds.max(30));
            Err(e.code.into())
        }
    }
}
fn write_metrics(
    db: &rusqlite::Connection,
    id: &str,
    metrics: &[QuotaMetric],
) -> Result<(), String> {
    db.execute("DELETE FROM quota_metrics WHERE account_id=?1", [id])
        .map_err(|_| "观测保存失败")?;
    for m in metrics {
        db.execute("INSERT INTO quota_metrics(account_id,metric_id,label,kind,unit,remaining,limit_value,remaining_percent,resets_at,source,observed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![id,m.metric_id,m.label,m.kind,m.unit,m.remaining,m.limit,m.remaining_percent,m.resets_at,m.source,m.observed_at]).map_err(|_|"观测保存失败")?;
    }
    let json = serde_json::to_string(metrics).map_err(|_| "观测保存失败")?;
    db.execute(
        "INSERT INTO observations(account_id,observed_at,metrics_json) VALUES(?1,?2,?3)",
        params![id, metrics[0].observed_at, json],
    )
    .map_err(|_| "历史保存失败")?;
    Ok(())
}
pub fn save(state: &AppState, session_id: &str) -> Result<crate::domain::models::Account, String> {
    let mut sessions = state
        .runtime
        .sessions
        .lock()
        .map_err(|_| "配置会话不可用")?;
    let s = sessions
        .get(session_id)
        .filter(|s| s.expires > Instant::now())
        .ok_or("配置已过期，请重新测试")?;
    if s.network_revision != state.runtime.network.revision() {
        return Err("网络设置已更改，请重新打开连接并测试".into());
    }
    if s.busy {
        return Err("读取尚未结束".into());
    }
    let metrics = s
        .probe
        .as_ref()
        .filter(|(at, _)| at.elapsed() < Duration::from_secs(300))
        .map(|(_, m)| m.clone());
    if s.input.enabled && metrics.is_none() {
        return Err("启用采集前必须测试读取成功".into());
    }
    let is_cli = get_provider(&s.input.provider_id).is_some_and(|p| !p.requires_credential());
    if is_cli && s.input.enabled && !s.input.cli_authorized {
        return Err("请先授权本地登录来源查询".into());
    }
    if !is_cli && s.key.is_none() && s.old_ref.is_none() {
        return Err("请配置 API Key".into());
    }
    let id = s
        .input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let prior_success = if s.key.is_none() {
        accounts::get(&db_lock(state)?.production, &id)?.and_then(|a| a.last_success_at)
    } else {
        None
    };
    let reference = if is_cli {
        String::new()
    } else if s.key.is_some() {
        Uuid::new_v4().to_string()
    } else {
        s.old_ref.clone().unwrap()
    };
    let revision = Uuid::new_v4().to_string();
    let mut db = db_lock(state)?;
    if s.network_revision != state.runtime.network.revision() {
        return Err("网络设置已更改，请重新打开连接并测试".into());
    }
    if is_cli {
        let duplicate: bool = db
            .production
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE provider_id=?1 AND id<>?2)",
                params![s.input.provider_id, id],
                |r| r.get(0),
            )
            .map_err(|_| "CLI 来源检查失败")?;
        if duplicate {
            return Err("此本地来源共用一个登录账号，请编辑该产品已有连接".into());
        }
    }
    if let Some(expected) = &s.old_revision {
        let current: Option<String> = db
            .production
            .query_row(
                "SELECT revision FROM connections WHERE account_id=?1",
                [&id],
                |r| r.get(0),
            )
            .ok();
        if current.as_ref() != Some(expected) {
            return Err("连接已更改或删除，请重新打开配置".into());
        }
    }
    if let Some(key) = &s.key {
        db.production
            .execute(
                "INSERT OR IGNORE INTO credential_cleanup VALUES(?1)",
                [&reference],
            )
            .map_err(|_| "凭据清理登记失败")?;
        credentials::write(&reference, key)?;
    }
    let result = (|| -> Result<(), String> {
        let tx = db.production.transaction().map_err(|_| "保存事务失败")?;
        tx.execute("INSERT INTO accounts(id,provider_id,alias,product_name,enabled,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(id) DO UPDATE SET alias=excluded.alias,enabled=excluded.enabled,updated_at=excluded.updated_at",params![id,s.input.provider_id,s.input.alias.trim(),get_provider(&s.input.provider_id).unwrap().descriptor().name,s.input.enabled,chrono::Utc::now().to_rfc3339()]).map_err(|_|"账号保存失败")?;
        tx.execute("INSERT INTO connections(account_id,base_url,credential_ref,revision,cli_authorized,network_json) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(account_id) DO UPDATE SET base_url=excluded.base_url,credential_ref=excluded.credential_ref,revision=excluded.revision,cli_authorized=excluded.cli_authorized,network_json=excluded.network_json,next_retry_at=NULL",params![id,s.input.base_url,reference,revision,is_cli && s.input.cli_authorized,serde_json::to_string(&s.input.network).map_err(|_|"网络设置无效")?]).map_err(|_|"连接保存失败")?;
        if s.key.is_some() {
            tx.execute("DELETE FROM quota_metrics WHERE account_id=?1", [&id])
                .map_err(|_| "旧观测清理失败")?;
            tx.execute("DELETE FROM observations WHERE account_id=?1", [&id])
                .map_err(|_| "旧历史清理失败")?;
        }
        if let Some(metrics) = &metrics {
            write_metrics(&tx, &id, metrics)?;
        }
        let success = metrics
            .as_ref()
            .map(|m| m[0].observed_at.clone())
            .or(prior_success);
        tx.execute("INSERT INTO fetch_states(account_id,status,last_success_at) VALUES(?1,?2,?3) ON CONFLICT(account_id) DO UPDATE SET status=excluded.status,last_success_at=excluded.last_success_at,error_code=NULL",params![id,if s.input.enabled{"ok"}else{"paused"},success]).map_err(|_|"采集状态保存失败")?;
        tx.execute(
            "DELETE FROM credential_cleanup WHERE credential_ref=?1",
            [&reference],
        )
        .map_err(|_| "凭据登记失败")?;
        if s.key.is_some() {
            if let Some(old) = &s.old_ref {
                tx.execute("INSERT OR IGNORE INTO credential_cleanup VALUES(?1)", [old])
                    .map_err(|_| "凭据清理登记失败")?;
            }
        }
        tx.commit().map_err(|_| "保存事务失败")?;
        Ok(())
    })();
    if result.is_err() && s.key.is_some() {
        let _ = credentials::remove(&reference);
    }
    result?;
    let account = accounts::get(&db.production, &id)?.ok_or("保存后账号不可用")?;
    sessions.remove(session_id);
    if let Ok(mut gates) = state.runtime.gates.lock() {
        gates.remove(&id);
    }
    Ok(account)
}
pub fn remove(state: &AppState, id: &str) -> Result<bool, String> {
    let mut db = db_lock(state)?;
    let tx = db.production.transaction().map_err(|_| "删除事务失败")?;
    let reference: Option<String> = tx
        .query_row(
            "SELECT credential_ref FROM connections WHERE account_id=?1",
            [id],
            |r| r.get(0),
        )
        .ok();
    if let Some(reference) = reference.filter(|r| !r.is_empty()) {
        tx.execute(
            "INSERT OR IGNORE INTO credential_cleanup VALUES(?1)",
            [reference],
        )
        .map_err(|_| "凭据清理登记失败")?;
    }
    let removed = accounts::remove(&tx, id)?;
    tx.commit().map_err(|_| "删除失败")?;
    Ok(removed)
}
pub fn cleanup(state: &AppState) {
    let references=match db_lock(state).and_then(|db|{
  let mut stmt=db.production.prepare("SELECT credential_ref FROM credential_cleanup WHERE credential_ref NOT IN (SELECT credential_ref FROM connections)").map_err(|_|"清理读取失败")?;
  let rows=stmt.query_map([],|r|r.get::<_,String>(0)).map_err(|_|"清理读取失败")?;
  rows.collect::<Result<Vec<_>,_>>().map_err(|_|"清理读取失败".into())
 }){Ok(v)=>v,Err(_)=>return};
    for reference in references {
        // Serialize cleanup with save so a pending reference cannot become live while being deleted.
        if let Ok(db) = db_lock(state) {
            let live: bool = db
                .production
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM connections WHERE credential_ref=?1)",
                    [&reference],
                    |r| r.get(0),
                )
                .unwrap_or(true);
            if !live && credentials::remove(&reference).is_ok() {
                let _ = db.production.execute(
                    "DELETE FROM credential_cleanup WHERE credential_ref=?1",
                    [&reference],
                );
            }
        }
    }
}
fn manual_wait(
    account: &crate::domain::models::Account,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let short_wait = account
        .last_attempt_at
        .as_deref()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
        .is_some_and(|date| now.timestamp() - date.timestamp() < 30);
    let rate_wait = account.fetch_status == "rate_limited"
        && account
            .next_retry_at
            .as_deref()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .is_some_and(|date| date.timestamp() > now.timestamp());
    short_wait || rate_wait
}

/// 所有入口共享同一后台、gates、并发与持久化；事件适配留在 host。
pub async fn refresh(app: tauri::AppHandle, id: String, manual: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    refresh_backend(
        &state,
        id,
        manual,
        None,
        &|state| {
            crate::commands::publish(&app, state);
        },
        &|ids| {
            let _ = app.emit("alerts-changed", ids);
        },
    )
    .await
}
pub async fn refresh_agent<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    grant: String,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let guard = || {
        let db = db_lock(&state).map_err(|_| FetchError::new("database_error"))?;
        let access = stillenough_core::access::load(&db.production);
        if access.allows(true, false) && access.updated_at.as_deref() == Some(grant.as_str()) {
            Ok(())
        } else {
            Err(FetchError::new("agent_access_denied"))
        }
    };
    refresh_backend(
        &state,
        id,
        true,
        Some(&guard),
        &|state| {
            crate::commands::publish(&app, state);
        },
        &|ids| {
            let _ = app.emit("alerts-changed", ids);
        },
    )
    .await
}
async fn refresh_backend(
    state: &AppState,
    id: String,
    manual: bool,
    guard: Option<AccessGuard<'_>>,
    publish: &(dyn Fn(&AppState) + Send + Sync),
    emit_alerts: &(dyn Fn(Vec<i64>) + Send + Sync),
) -> Result<(), String> {
    if let Some(check) = guard {
        check().map_err(|e| e.code.to_string())?;
    }
    let configuration = {
        let db = db_lock(state)?;
        let account = accounts::get(&db.production, &id)?.ok_or("not_found")?;
        if !account.enabled {
            return Err("account_disabled".into());
        }
        let retry_at: Option<String> = db
            .production
            .query_row(
                "SELECT next_retry_at FROM connections WHERE account_id=?1",
                [&id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        if account.fetch_status == "rate_limited"
            && retry_at
                .as_deref()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .is_some_and(|d| d > chrono::Utc::now())
        {
            return Err("rate_limited".into());
        }
        if (manual && manual_wait(&account, chrono::Utc::now()))
            || (!manual
                && retry_at
                    .as_deref()
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                    .is_some_and(|date| date.timestamp() > chrono::Utc::now().timestamp()))
        {
            return Err("refresh_cooldown".into());
        }
        let (url, reference, revision) = db
            .production
            .query_row(
                "SELECT base_url,credential_ref,revision FROM connections WHERE account_id=?1",
                [&id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(|_| "连接不存在")?;
        (
            AccountInput {
                id: Some(id.clone()),
                provider_id: account.provider_id,
                alias: account.alias,
                base_url: url,
                enabled: true,
                cli_authorized: account.cli_authorized,
                network: account.network,
            },
            reference,
            revision,
        )
    };
    {
        let mut gates = state.runtime.gates.lock().map_err(|_| "调度不可用")?;
        let gate = gates.entry(id.clone()).or_insert(Gate {
            busy: false,
            next_attempt: Instant::now(),
            failures: 0,
        });
        if gate.busy {
            return Err("refresh_in_progress".into());
        }
        if !manual && remaining_wait(gate.next_attempt) {
            return Err("refresh_cooldown".into());
        }
        gate.busy = true;
    }
    publish(state);
    let configuration_guard = || {
        if let Some(check) = guard {
            check()?;
        }
        let db = db_lock(state).map_err(|_| FetchError::new("database_error"))?;
        let current = accounts::get(&db.production, &id)
            .map_err(|_| FetchError::new("database_error"))?
            .ok_or_else(|| FetchError::new("not_found"))?;
        if !current.enabled {
            return Err(FetchError::new("account_disabled"));
        }
        if get_provider(&current.provider_id).is_some_and(|p| !p.requires_credential())
            && !current.cli_authorized
        {
            return Err(FetchError::new("cli_authorization_required"));
        }
        let revision: String = db
            .production
            .query_row(
                "SELECT revision FROM connections WHERE account_id=?1",
                [&id],
                |row| row.get(0),
            )
            .map_err(|_| FetchError::new("not_found"))?;
        if revision != configuration.2 {
            return Err(FetchError::new("not_found"));
        }
        Ok(())
    };
    let guard: Option<AccessGuard<'_>> = Some(&configuration_guard);
    let reference = configuration.1;
    let is_cli =
        get_provider(&configuration.0.provider_id).is_some_and(|p| !p.requires_credential());
    let permitted = guard.map(|check| check()).transpose();
    #[cfg(test)]
    let synthetic = state.runtime.test_fetch.lock().unwrap().is_some();
    #[cfg(not(test))]
    let synthetic = false;
    let key = if let Err(e) = permitted {
        Err(e.code.into())
    } else if is_cli || synthetic {
        Ok(Zeroizing::new(String::new()))
    } else {
        tokio::task::spawn_blocking(move || credentials::read(&reference))
            .await
            .map_err(|_| "auth_required".to_string())
            .and_then(|r| r)
    };
    let result = match key {
        Ok(key) => fetch_guarded(&state.runtime, &configuration.0, &key, guard).await,
        Err(e) => Err(FetchError::new(stillenough_core::errors::classify(&e).0)),
    };
    if result.as_ref().is_err_and(|e| {
        matches!(
            e.code,
            "agent_access_denied" | "account_disabled" | "cli_authorization_required" | "not_found"
        )
    }) {
        if let Ok(mut gates) = state.runtime.gates.lock() {
            if let Some(gate) = gates.get_mut(&id) {
                gate.busy = false;
            }
        }
        publish(state);
        return Err(result
            .err()
            .map(|e| e.code)
            .unwrap_or("internal_error")
            .into());
    }
    let retry = {
        let minutes = state
            .settings
            .lock()
            .map(|s| s.refresh_minutes as u64)
            .unwrap_or(10);
        let mut gates = state.runtime.gates.lock().map_err(|_| "调度不可用")?;
        let gate = gates.get_mut(&id).ok_or("调度不可用")?;
        let seconds = match &result {
            Ok(_) => {
                gate.failures = 0;
                minutes * 60
            }
            Err(e) => {
                gate.failures += 1;
                if [
                    "auth_required",
                    "permission_denied",
                    "schema_changed",
                    "unsupported",
                ]
                .contains(&e.code)
                {
                    86400
                } else {
                    [60, 120, 300, 900, 1800][(gate.failures - 1).min(4)].max(e.retry_after_seconds)
                }
            }
        };
        gate.next_attempt = Instant::now() + Duration::from_secs(seconds);
        seconds
    };
    let returned_error = result.as_ref().err().map(|e| e.code.to_string());
    let monitoring = state
        .settings
        .lock()
        .map(|settings| MonitoringSettings::from(&*settings))
        .unwrap_or_default();
    let persisted = (|| {
        let mut db = db_lock(state)?;
        persist_outcome(
            &mut db.production,
            &id,
            &configuration.2,
            result,
            retry,
            monitoring,
        )
    })();
    if let Ok(mut gates) = state.runtime.gates.lock() {
        if let Some(gate) = gates.get_mut(&id) {
            gate.busy = false;
        }
    }
    publish(state);
    let created = persisted?;
    if !created.is_empty() {
        // The reminder exists only while the app can show it; nothing here claims a
        // delivery that would have had to happen while the process was closed.
        emit_alerts(created);
    }
    if let Some(error) = returned_error {
        Err(error)
    } else {
        Ok(())
    }
}
fn persist_outcome(
    db: &mut rusqlite::Connection,
    id: &str,
    revision: &str,
    result: Result<Vec<QuotaMetric>, FetchError>,
    retry: u64,
    monitoring: MonitoringSettings,
) -> Result<Vec<i64>, String> {
    let current: Option<String> = db
        .query_row(
            "SELECT revision FROM connections WHERE account_id=?1",
            [id],
            |r| r.get(0),
        )
        .ok();
    if current.as_deref() != Some(revision) {
        return Err("连接已更改或删除，丢弃旧查询结果".into());
    }
    let tx = db.transaction().map_err(|_| "观测事务失败")?;
    let mut created = Vec::new();
    match result {
        Ok(metrics) => {
            write_metrics(&tx, id, &metrics)?;
            // Alerts are evaluated on the values just written, inside the same transaction.
            created = alerts::evaluate(
                &tx,
                id,
                &metrics,
                monitoring.alert_enabled,
                &monitoring.alert_thresholds,
            )?;
            tx.execute("UPDATE fetch_states SET status='ok',attempt_at=?2,last_success_at=?3,error_code=NULL WHERE account_id=?1",params![id,chrono::Utc::now().to_rfc3339(),metrics[0].observed_at]).map_err(|_|"状态保存失败")?;
        }
        Err(e) => {
            tx.execute(
                "UPDATE fetch_states SET status=?2,attempt_at=?3,error_code=?2 WHERE account_id=?1",
                params![
                    id,
                    stillenough_core::errors::classify(e.code).0,
                    chrono::Utc::now().to_rfc3339()
                ],
            )
            .map_err(|_| "状态保存失败")?;
        }
    }
    tx.execute(
        "UPDATE connections SET next_retry_at=?2 WHERE account_id=?1",
        params![
            id,
            (chrono::Utc::now() + chrono::Duration::seconds(retry.min(i64::MAX as u64) as i64))
                .to_rfc3339()
        ],
    )
    .map_err(|_| "重试时间保存失败")?;
    history::prune(&tx, monitoring.history_retention_days)?;
    tx.commit().map_err(|_| "观测保存失败")?;
    Ok(created)
}
/// Accounts a refresh pass may touch right now, honouring the global pause.
fn schedulable(state: &AppState, minutes: u8) -> Vec<String> {
    if state
        .settings
        .lock()
        .map(|settings| settings.collection_paused)
        .unwrap_or(false)
    {
        return vec![];
    }
    let Ok(db) = db_lock(state) else {
        return vec![];
    };
    let Ok(list) = accounts::list(&db.production) else {
        return vec![];
    };
    list.into_iter()
        .filter(|a| {
            a.enabled
                && a.base_url.is_some()
                && ![
                    "auth_required",
                    "permission_denied",
                    "schema_changed",
                    "unsupported",
                ]
                .contains(&a.fetch_status.as_str())
                && a.last_success_at
                    .as_ref()
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                    .is_none_or(|date| {
                        chrono::Utc::now().timestamp() - date.timestamp() >= minutes as i64 * 60
                    })
        })
        .map(|a| a.id)
        .collect()
}

/// Refreshes every account that is currently able to be queried.
pub async fn refresh_all(app: tauri::AppHandle, manual: bool) {
    let ids = {
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let state = handle.state::<AppState>();
            let Ok(settings) = state.settings.lock() else {
                return vec![];
            };
            if settings.collection_paused {
                return vec![];
            }
            let minutes = settings.refresh_minutes;
            drop(settings);
            schedulable(&state, minutes)
        })
        .await
        .unwrap_or_default()
    };
    for id in ids {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = refresh(handle, id, manual).await;
        });
    }
}

/// How long the wall clock ran ahead of the monotonic clock since the last tick.
/// A sleeping machine advances neither, but the system clock jumps forward, so
/// the gap is the sleep duration and is what tells us the schedule was skipped.
fn suspended_seconds(
    last_wall: &mut std::time::SystemTime,
    last_tick: &mut std::time::Instant,
) -> u64 {
    let now_wall = std::time::SystemTime::now();
    let now_tick = std::time::Instant::now();
    let wall_elapsed = now_wall
        .duration_since(*last_wall)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let tick_elapsed = now_tick.duration_since(*last_tick).as_secs();
    *last_wall = now_wall;
    *last_tick = now_tick;
    wall_elapsed.saturating_sub(tick_elapsed)
}

pub fn start_scheduler(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_wall = std::time::SystemTime::now();
        let mut last_tick = std::time::Instant::now();
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            let handle = app.clone();
            let ids = tauri::async_runtime::spawn_blocking(move || {
                let state = handle.state::<AppState>();
                state.runtime.prune();
                cleanup(&state);
                let minutes = state
                    .settings
                    .lock()
                    .map(|s| s.refresh_minutes)
                    .unwrap_or(10);
                schedulable(&state, minutes)
            })
            .await
            .unwrap_or_default();
            for id in ids {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = refresh(handle, id, false).await;
                });
            }
            tokio::time::sleep(Duration::from_secs(15)).await;
            // A long sleep freezes the loop, so the next tick would wait out a
            // full interval before showing numbers that are now hours old.
            if suspended_seconds(&mut last_wall, &mut last_tick) > 90 {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    refresh_all(handle, true).await;
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_sleeping_machine_is_detected_as_wall_clock_running_ahead_of_the_tick_clock() {
        // The stored wall clock is an hour behind while the monotonic clock has
        // barely moved: exactly what a machine that slept for an hour looks like.
        let mut wall = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let mut tick = std::time::Instant::now();
        let detected = suspended_seconds(&mut wall, &mut tick);
        assert!(
            detected >= 3500,
            "1 小时的挂起应被识别，实际识别为 {detected} 秒"
        );
    }

    #[test]
    fn a_normal_poll_interval_is_not_mistaken_for_sleep() {
        let mut wall = std::time::SystemTime::now();
        let mut tick = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let detected = suspended_seconds(&mut wall, &mut tick);
        assert!(
            detected < 90,
            "正常轮询不应触发挂起恢复，实际 {detected} 秒"
        );
    }

    use super::*;
    #[test]
    fn manual_refresh_bypasses_schedule_and_network_backoff_but_respects_rate_limit() {
        let now = chrono::Utc::now();
        let mut account = crate::domain::models::Account {
            id: "test".into(),
            provider_id: "test".into(),
            alias: "test".into(),
            product_name: "test".into(),
            enabled: true,
            metrics: vec![],
            fetch_status: "ok".into(),
            last_success_at: Some(now.to_rfc3339()),
            last_attempt_at: Some((now - chrono::Duration::seconds(31)).to_rfc3339()),
            next_retry_at: Some((now + chrono::Duration::minutes(10)).to_rfc3339()),
            refreshing: false,
            base_url: None,
            cli_authorized: false,
            network: Default::default(),
        };
        assert!(!manual_wait(&account, now));
        account.fetch_status = "network_error".into();
        assert!(!manual_wait(&account, now));
        account.fetch_status = "rate_limited".into();
        assert!(manual_wait(&account, now));
        assert!(!manual_wait(&account, now + chrono::Duration::minutes(10)));
        account.fetch_status = "ok".into();
        account.last_attempt_at = Some((now - chrono::Duration::seconds(29)).to_rfc3339());
        assert!(manual_wait(&account, now));
        assert!(!manual_wait(&account, now + chrono::Duration::seconds(1)));
    }

    #[tokio::test]
    async fn provider_cooldown_does_not_block_another_provider() {
        let runtime = Runtime::new().unwrap();
        runtime.provider_cooldown.lock().unwrap().insert(
            "other:https://other.invalid".into(),
            Instant::now() + Duration::from_secs(120),
        );
        let mut request = input();
        request.provider_id = "unregistered".into();
        assert_eq!(
            fetch(&runtime, &request, "").await.unwrap_err().code,
            "unsupported"
        );
        runtime.provider_cooldown.lock().unwrap().insert(
            format!("{}:{}", request.provider_id, request.base_url),
            Instant::now() + Duration::from_secs(120),
        );
        let error = fetch(&runtime, &request, "").await.unwrap_err();
        assert_eq!(error.code, "rate_limited");
        assert!(error.retry_after_seconds > 0);
    }

    fn state() -> AppState {
        fn db() -> rusqlite::Connection {
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
            db.execute_batch(include_str!("../../migrations/006_network.sql"))
                .unwrap();
            db
        }
        AppState {
            databases: Mutex::new(crate::commands::Databases { production: db() }),
            settings: Mutex::new(Default::default()),
            runtime: Runtime::new().unwrap(),
            revision: std::sync::atomic::AtomicU64::new(0),
        }
    }
    fn input() -> AccountInput {
        AccountInput {
            id: None,
            provider_id: "deepseek_api".into(),
            alias: "test".into(),
            base_url: "https://api.deepseek.com".into(),
            enabled: true,
            cli_authorized: false,
            network: Default::default(),
        }
    }
    #[test]
    fn global_network_change_invalidates_previously_successful_setup() {
        let state = state();
        let session = begin(&state, input()).unwrap();
        state
            .runtime
            .sessions
            .lock()
            .unwrap()
            .get_mut(&session)
            .unwrap()
            .probe = Some((Instant::now(), vec![]));
        let db = state.databases.lock().unwrap();
        state
            .runtime
            .network
            .save(
                &db.production,
                super::super::network::NetworkConfig {
                    mode: "direct".into(),
                    proxy_url: String::new(),
                },
            )
            .unwrap();
        drop(db);
        assert!(save(&state, &session)
            .unwrap_err()
            .contains("网络设置已更改"));
    }
    #[test]
    fn canceled_expired_or_untested_credentials_cannot_enable() {
        let state = state();
        let session = begin(&state, input()).unwrap();
        set_key(
            &state,
            &session,
            Zeroizing::new("synthetic-not-a-real-key".into()),
        )
        .unwrap();
        assert!(save(&state, &session).is_err());
        assert!(accounts::list(&state.databases.lock().unwrap().production)
            .unwrap()
            .is_empty());
        state
            .runtime
            .sessions
            .lock()
            .unwrap()
            .get_mut(&session)
            .unwrap()
            .expires = Instant::now() - Duration::from_secs(1);
        state.runtime.prune();
        assert!(state.runtime.sessions.lock().unwrap().is_empty());
        let session = begin(&state, input()).unwrap();
        cancel(&state, &session);
        assert!(save(&state, &session).is_err());
    }
    #[tokio::test]
    async fn subscription_sources_require_consent_before_access() {
        let state = state();
        for (provider, source) in [
            ("codex", "cli://codex/subscription"),
            ("codex_http", "local://codex/subscription-http"),
            ("claude_code", "local://claude-code/subscription"),
        ] {
            let mut local = input();
            local.provider_id = provider.into();
            local.base_url = source.into();
            assert_eq!(
                fetch(&state.runtime, &local, "").await.unwrap_err().code,
                "cli_authorization_required"
            );
            local.cli_authorized = true;
            let session = begin(&state, local).unwrap();
            assert!(set_key(&state, &session, Zeroizing::new("synthetic-token".into())).is_err());
            assert!(save(&state, &session).is_err());
            cancel(&state, &session);
        }
    }
    #[tokio::test]
    async fn cli_requires_consent_and_saves_without_credentials() {
        let state = state();
        let mut cli = input();
        cli.provider_id = "bailian_token_plan".into();
        cli.base_url = crate::providers::bailian_cli::SOURCE.into();
        assert_eq!(
            fetch(&state.runtime, &cli, "").await.unwrap_err().code,
            "cli_authorization_required"
        );
        cli.cli_authorized = true;
        let session = begin(&state, cli.clone()).unwrap();
        assert!(set_key(&state, &session, Zeroizing::new("synthetic-token".into())).is_err());
        assert!(save(&state, &session).is_err());
        let metrics = crate::providers::bailian_cli::parse(
            include_bytes!("../../../fixtures/providers/bailian-token-plan-cli.json"),
            "2026-10-01T00:00:00Z",
        )
        .unwrap();
        state
            .runtime
            .sessions
            .lock()
            .unwrap()
            .get_mut(&session)
            .unwrap()
            .probe = Some((Instant::now(), metrics));
        let saved = save(&state, &session).unwrap();
        assert!(saved.cli_authorized);
        {
            let db = state.databases.lock().unwrap();
            assert_eq!(
                db.production
                    .query_row("SELECT credential_ref FROM connections", [], |r| r
                        .get::<_, String>(0))
                    .unwrap(),
                ""
            );
            assert_eq!(
                db.production
                    .query_row("SELECT COUNT(*) FROM credential_cleanup", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        cli.id = Some(saved.id.clone());
        cli.cli_authorized = false;
        cli.enabled = false;
        let session = begin(&state, cli).unwrap();
        let revoked = save(&state, &session).unwrap();
        assert!(!revoked.enabled && !revoked.cli_authorized);
        assert_eq!(revoked.metrics.len(), saved.metrics.len());
        remove(&state, &saved.id).unwrap();
        assert_eq!(
            state
                .databases
                .lock()
                .unwrap()
                .production
                .query_row("SELECT COUNT(*) FROM credential_cleanup", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn a_successful_refresh_records_one_alert_and_prunes_by_configured_retention() {
        let state = state();
        let mut db = state.databases.lock().unwrap();
        db.production.execute("INSERT INTO accounts VALUES('real','deepseek_api','test','DeepSeek API',1,'now','now')",[]).unwrap();
        db.production.execute("INSERT INTO connections(account_id,base_url,credential_ref,revision) VALUES('real','https://api.deepseek.com','opaque','r')",[]).unwrap();
        db.production
            .execute(
                "INSERT INTO fetch_states(account_id,status) VALUES('real','ok')",
                [],
            )
            .unwrap();
        let quota = |percent: f64, observed_at: &str| {
            vec![QuotaMetric {
                metric_id: "quota_5h".into(),
                label: "5 小时窗口".into(),
                kind: "quota".into(),
                unit: "%".into(),
                remaining: Some(format!("{percent:.1}%")),
                limit: None,
                remaining_percent: Some(percent),
                resets_at: None,
                source: "official_api".into(),
                observed_at: observed_at.into(),
                stale_after_seconds: 1200,
            }]
        };
        let monitoring = MonitoringSettings::default();
        let old = (chrono::Utc::now() - chrono::Duration::days(120)).to_rfc3339();
        db.production.execute("INSERT INTO observations(account_id,observed_at,metrics_json) VALUES('real',?1,'[]')", [&old]).unwrap();

        assert!(
            persist_outcome(
                &mut db.production,
                "real",
                "r",
                Ok(quota(40.0, "2026-10-01T00:00:00Z")),
                600,
                monitoring.clone()
            )
            .unwrap()
            .is_empty(),
            "首次观测只布防"
        );
        let fired = persist_outcome(
            &mut db.production,
            "real",
            "r",
            Ok(quota(4.0, "2026-10-01T01:00:00Z")),
            600,
            monitoring.clone(),
        )
        .unwrap();
        assert_eq!(fired.len(), 1, "跌破 5% 产生一条提醒");
        assert!(
            persist_outcome(
                &mut db.production,
                "real",
                "r",
                Ok(quota(3.0, "2026-10-01T02:00:00Z")),
                600,
                monitoring.clone()
            )
            .unwrap()
            .is_empty(),
            "同一 episode 不重复提醒"
        );
        assert_eq!(
            db.production
                .query_row("SELECT COUNT(*) FROM alert_events", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(alerts::unread_count(&db.production).unwrap(), 1);

        let mut short = monitoring.clone();
        short.history_retention_days = 30;
        persist_outcome(
            &mut db.production,
            "real",
            "r",
            Ok(quota(50.0, "2026-10-01T03:00:00Z")),
            600,
            short,
        )
        .unwrap();
        let oldest: String = db
            .production
            .query_row("SELECT MIN(observed_at) FROM observations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(oldest > old, "超过保留期的观测已清理");
    }

    #[test]
    fn failed_query_and_deleted_connection_preserve_real_state() {
        let state = state();
        let mut db = state.databases.lock().unwrap();
        db.production.execute("INSERT INTO accounts VALUES('real','deepseek_api','test','DeepSeek API',1,'now','now')",[]).unwrap();
        let metrics = crate::providers::deepseek::parse(
            include_bytes!("../../../fixtures/providers/deepseek-balance.json"),
            "2026-10-01T00:00:00Z",
        )
        .unwrap();
        let tx = db.production.transaction().unwrap();
        write_metrics(&tx, "real", &metrics).unwrap();
        tx.commit().unwrap();
        db.production.execute("INSERT INTO fetch_states(account_id,status,last_success_at) VALUES('real','network_error','2026-10-01T00:00:00Z')",[]).unwrap();
        db.production.execute("INSERT INTO connections(account_id,base_url,credential_ref,revision) VALUES('real','https://api.deepseek.com','test-ref','revision')",[]).unwrap();
        persist_outcome(
            &mut db.production,
            "real",
            "revision",
            Err(FetchError::new("network_error")),
            120,
            Default::default(),
        )
        .unwrap();
        assert!(db
            .production
            .query_row("SELECT next_retry_at FROM connections", [], |r| r
                .get::<_, Option<String>>(0))
            .unwrap()
            .is_some());
        let account = accounts::get(&db.production, "real").unwrap().unwrap();
        assert_eq!(account.metrics[0].observed_at, "2026-10-01T00:00:00Z");
        assert_eq!(
            account.last_success_at.as_deref(),
            Some("2026-10-01T00:00:00Z")
        );
        assert_eq!(
            db.production
                .query_row("SELECT COUNT(*) FROM observations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(account.metrics[0].source, "official_api");
        accounts::remove(&db.production, "real").unwrap();
        assert!(persist_outcome(
            &mut db.production,
            "real",
            "revision",
            Ok(metrics),
            30,
            Default::default()
        )
        .is_err());
        assert!(accounts::list(&db.production).unwrap().is_empty());
    }

    #[test]
    fn real_connections_and_retry_state_persist_across_reopen() {
        let path = std::env::temp_dir().join(format!("apm-real-{}.sqlite", Uuid::new_v4()));
        {
            let mut db = crate::infra::sqlite::open_database(&path).unwrap();
            db.execute("INSERT INTO accounts VALUES('real','deepseek_api','test','DeepSeek API',1,'now','now')",[]).unwrap();
            db.execute("INSERT INTO connections(account_id,base_url,credential_ref,revision) VALUES('real','https://api.deepseek.com','opaque-test-ref','r')",[]).unwrap();
            db.execute(
                "INSERT INTO fetch_states(account_id,status) VALUES('real','idle')",
                [],
            )
            .unwrap();
            let metrics = crate::providers::deepseek::parse(
                include_bytes!("../../../fixtures/providers/deepseek-balance.json"),
                &chrono::Utc::now().to_rfc3339(),
            )
            .unwrap();
            persist_outcome(&mut db, "real", "r", Ok(metrics), 120, Default::default()).unwrap();
        }
        {
            let db = crate::infra::sqlite::open_database(&path).unwrap();
            let account = accounts::get(&db, "real").unwrap().unwrap();
            assert_eq!(
                account.base_url.as_deref(),
                Some("https://api.deepseek.com")
            );
            assert_eq!(account.metrics.len(), 2);
            assert!(account.last_success_at.is_some());
            assert!(db
                .query_row("SELECT next_retry_at FROM connections", [], |r| r
                    .get::<_, Option<String>>(0))
                .unwrap()
                .is_some());
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    #[ignore = "writes a synthetic credential to Windows Credential Manager; run explicitly"]
    fn windows_save_replace_delete_and_no_secret_in_sqlite() {
        let state = state();
        let session = begin(&state, input()).unwrap();
        let key = "synthetic-not-a-real-api-key";
        set_key(&state, &session, Zeroizing::new(key.into())).unwrap();
        let metrics = crate::providers::deepseek::parse(
            include_bytes!("../../../fixtures/providers/deepseek-balance.json"),
            "2026-10-01T00:00:00Z",
        )
        .unwrap();
        state
            .runtime
            .sessions
            .lock()
            .unwrap()
            .get_mut(&session)
            .unwrap()
            .probe = Some((Instant::now(), metrics));
        let account = save(&state, &session).unwrap();
        let reference: String = state
            .databases
            .lock()
            .unwrap()
            .production
            .query_row("SELECT credential_ref FROM connections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(&*credentials::read(&reference).unwrap(), key);
        let json = serde_json::to_string(&account).unwrap();
        assert!(!json.contains(key));
        assert!(!json.contains(&reference));
        let db_value: String = state
            .databases
            .lock()
            .unwrap()
            .production
            .query_row("SELECT metrics_json FROM observations LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(!db_value.contains(key));
        assert_ne!(reference, key);
        let mut replacement = input();
        replacement.id = Some(account.id.clone());
        replacement.enabled = false;
        let session = begin(&state, replacement).unwrap();
        set_key(
            &state,
            &session,
            Zeroizing::new("synthetic-replacement-key".into()),
        )
        .unwrap();
        let changed = save(&state, &session).unwrap();
        assert!(changed.metrics.is_empty());
        assert!(changed.last_success_at.is_none());
        cleanup(&state);
        assert!(credentials::read(&reference).is_err());
        remove(&state, &account.id).unwrap();
        cleanup(&state);
        assert_eq!(
            state
                .databases
                .lock()
                .unwrap()
                .production
                .query_row("SELECT COUNT(*) FROM credential_cleanup", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
