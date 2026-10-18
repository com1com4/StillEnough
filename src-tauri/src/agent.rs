use crate::{
    commands::{publish, require_main, AppState},
    services::connections,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use stillenough_core::{
    access::{self, AgentAccess},
    errors,
    protocol::Response,
    request::{Command, Request},
    storage,
};
use tauri::{AppHandle, Manager, State};
#[derive(Clone)]
struct Operation {
    request: Request,
    grant: String,
    result: Option<Response>,
    created: Instant,
}
pub struct AgentRuntime {
    pub instance: String,
    operations: Mutex<HashMap<String, Operation>>,
}
impl Default for AgentRuntime {
    fn default() -> Self {
        Self {
            instance: uuid::Uuid::new_v4().to_string(),
            operations: Mutex::new(HashMap::new()),
        }
    }
}
#[tauri::command]
pub fn get_agent_access(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<AgentAccess, String> {
    require_main(&window)?;
    let db = state.databases.lock().map_err(|_| "database_error")?;
    Ok(access::load(&db.production))
}
#[tauri::command]
pub fn set_agent_access(
    mut value: AgentAccess,
    window: tauri::WebviewWindow,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AgentAccess, String> {
    require_main(&window)?;
    value.updated_at = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true));
    if !value.enabled {
        value.allow_refresh = false;
        value.allow_offline_cache = false;
    }
    let db = state.databases.lock().map_err(|_| "database_error")?;
    access::save(&db.production, &value)?;
    drop(db);
    publish(&app, &state);
    Ok(value)
}
fn grant<R: tauri::Runtime>(app: &AppHandle<R>, refresh: bool) -> Result<String, &'static str> {
    let state = app.state::<AppState>();
    let db = state.databases.lock().map_err(|_| "database_error")?;
    let access = access::load(&db.production);
    if !access.allows(refresh, false) {
        return Err("agent_access_denied");
    }
    access.updated_at.ok_or("agent_access_denied")
}
fn base<R: tauri::Runtime>(app: &AppHandle<R>, request: &Request) -> Response {
    let mut r = Response::local(request.command.name(), request.command.kind());
    r.request_id = request.request_id.clone();
    r.transport = "ipc".into();
    r.instance_id = Some(app.state::<AgentRuntime>().instance.clone());
    r.revision = Some(crate::commands::revision(&app.state::<AppState>()));
    r
}
fn snapshot<R: tauri::Runtime>(app: &AppHandle<R>, request: &Request) -> Response {
    let mut r = base(app, request);
    let result = (|| {
        let state = app.state::<AppState>();
        let ttl = (u64::from(
            state
                .settings
                .lock()
                .map_err(|_| "internal_error")?
                .refresh_minutes,
        ) * 120)
            .max(900);
        for _ in 0..3 {
            r.data.accounts.clear();
            r.data.checks.clear();
            r.errors.clear();
            r.warnings.clear();
            r.outcome = "ok".into();
            let before = crate::commands::revision(&state);
            let db = state.databases.lock().map_err(|_| "database_error")?;
            let tx = db
                .production
                .unchecked_transaction()
                .map_err(|_| "database_error")?;
            storage::schema(&tx)?;
            if !access::load(&tx).allows(request.refreshes(), false) {
                return Err("agent_access_denied");
            }
            storage::populate(&tx, request, &mut r)?;
            {
                let gates = state.runtime.gates.lock().map_err(|_| "internal_error")?;
                for a in &mut r.data.accounts {
                    a.refreshing = gates.get(&a.id).is_some_and(|g| g.busy);
                    if a.enabled && a.refreshing {
                        a.status = stillenough_core::protocol::Status::Busy;
                    }
                    for m in &mut a.metrics {
                        m.stale_after_seconds = ttl;
                    }
                }
            }
            tx.commit().map_err(|_| "database_error")?;
            let after = crate::commands::revision(&state);
            if before == after {
                r.revision = Some(after);
                return Ok(());
            }
        }
        Err("database_busy")
    })();
    if let Err(code) = result {
        r.data.accounts.clear();
        errors::fail(&mut r, code);
    }
    r
}
fn denied<R: tauri::Runtime>(app: &AppHandle<R>, request: &Request, code: &str) -> Response {
    let mut r = base(app, request);
    errors::fail(&mut r, code);
    r
}
pub async fn handle<R: tauri::Runtime>(app: AppHandle<R>, request: Request) -> Response {
    if let Err(code) = request.validate() {
        return denied(&app, &request, code);
    }
    if matches!(request.command, Command::Version | Command::Providers) || request.offline {
        return denied(&app, &request, "invalid_arguments");
    }
    let access = match grant(&app, request.refreshes()) {
        Ok(g) => g,
        Err(c) => return denied(&app, &request, c),
    };
    if request.command == Command::Operation {
        let runtime = app.state::<AgentRuntime>();
        let Ok(ops) = runtime.operations.lock() else {
            return denied(&app, &request, "internal_error");
        };
        let Some(op) = ops.get(request.operation_id.as_deref().unwrap_or("")) else {
            return denied(&app, &request, "operation_lost");
        };
        if op.grant != access {
            return denied(&app, &request, "agent_access_denied");
        }
        let mut r = op.result.clone().unwrap_or_else(|| base(&app, &request));
        r.command = request.command.name().into();
        r.request_id = request.request_id.clone();
        r.data.kind = "operation".into();
        r.data.operation_id = request.operation_id.clone();
        r.data.operation_state = Some(
            if op.result.is_none() {
                "running"
            } else if r.outcome == "ok" {
                "completed"
            } else {
                "failed"
            }
            .into(),
        );
        return r;
    }
    if !request.refreshes() {
        return snapshot(&app, &request);
    }
    let runtime = app.state::<AgentRuntime>();
    let inserted = {
        let Ok(mut ops) = runtime.operations.lock() else {
            return denied(&app, &request, "internal_error");
        };
        ops.retain(|_, op| op.created.elapsed().as_secs() < 3600 || op.result.is_none());
        if let Some(old) = ops.get(&request.request_id) {
            if serde_json::to_vec(&old.request).ok() != serde_json::to_vec(&request).ok()
                || old.grant != access
            {
                return denied(&app, &request, "invalid_arguments");
            }
            false
        } else {
            if ops.len() >= 128 || ops.values().filter(|v| v.result.is_none()).count() >= 8 {
                return denied(&app, &request, "refresh_in_progress");
            }
            ops.insert(
                request.request_id.clone(),
                Operation {
                    request: request.clone(),
                    grant: access.clone(),
                    result: None,
                    created: Instant::now(),
                },
            );
            true
        }
    };
    if inserted {
        let app = app.clone();
        let request = request.clone();
        let token = access.clone();
        tauri::async_runtime::spawn(async move {
            let result = execute_refresh(app.clone(), request.clone(), token).await;
            if let Ok(mut ops) = app.state::<AgentRuntime>().operations.lock() {
                if let Some(op) = ops.get_mut(&request.request_id) {
                    op.result = Some(result);
                }
            }
        });
    }
    loop {
        if grant(&app, true).as_deref() != Ok(access.as_str()) {
            return denied(&app, &request, "agent_access_denied");
        }
        if let Ok(ops) = runtime.operations.lock() {
            if let Some(r) = ops
                .get(&request.request_id)
                .and_then(|op| op.result.clone())
            {
                return r;
            }
        }
        if chrono::DateTime::parse_from_rfc3339(&request.deadline)
            .is_ok_and(|d| d <= chrono::Utc::now())
        {
            let mut r = denied(&app, &request, "deadline_exceeded");
            r.data.operation_id = Some(request.request_id.clone());
            r.data.operation_state = Some("running".into());
            return r;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
async fn execute_refresh<R: tauri::Runtime>(
    app: AppHandle<R>,
    request: Request,
    token: String,
) -> Response {
    let mut selection = request.clone();
    if request.command == Command::Doctor {
        selection.command = Command::Quota;
        selection.live = false;
    }
    let mut selected = snapshot(&app, &selection);
    selected.command = request.command.name().into();
    selected.data.kind = request.command.kind().into();
    if selected.outcome == "error" {
        selected.data.operation_id = Some(request.request_id.clone());
        selected.data.operation_state = Some("failed".into());
        return selected;
    }
    let ids: Vec<_> = selected
        .data
        .accounts
        .iter()
        .map(|a| a.id.clone())
        .collect();
    let mut succeeded = 0;
    let mut failures = Vec::new();
    if ids.is_empty() {
        selected.warnings.push("no_accounts".into());
    }
    let paused = app
        .state::<AppState>()
        .settings
        .lock()
        .map(|s| s.collection_paused)
        .unwrap_or(false);
    // 同一服务的 semaphore 保持全局并发 3；这里批量并发，不启动第二 Runtime。
    let mut jobs = Vec::new();
    for id in ids {
        let app = app.clone();
        let token = token.clone();
        jobs.push(tauri::async_runtime::spawn(async move {
            let result = connections::refresh_agent(app, id.clone(), token).await;
            (id, result)
        }));
    }
    for job in jobs {
        match job.await {
            Ok((_id, Ok(()))) => succeeded += 1,
            Ok((id, Err(code))) => failures.push(errors::safe_error(&code, Some(id))),
            Err(_) => failures.push(errors::safe_error("internal_error", None)),
        }
    }
    if grant(&app, true).as_deref() != Ok(token.as_str()) {
        return denied(&app, &request, "agent_access_denied");
    }
    let mut r = snapshot(&app, &request);
    r.warnings.extend(selected.warnings);
    if paused {
        r.warnings.push("collection_paused".into());
    }
    r.data.operation_id = Some(request.request_id.clone());
    r.data.operation_state = Some(
        if failures.is_empty() {
            "completed"
        } else {
            "failed"
        }
        .into(),
    );
    for e in &mut failures {
        if let Some(id) = &e.account_id {
            e.next_retry_at = r
                .data
                .accounts
                .iter()
                .find(|a| &a.id == id)
                .and_then(|a| a.next_retry_at.clone());
        }
    }
    if request.command == Command::Doctor {
        r.data.checks.push(stillenough_core::protocol::Check {
            name: "live-query".into(),
            state: if failures.is_empty() { "ok" } else { "error" }.into(),
            code: failures.first().map(|e| e.code.clone()),
        });
    }
    if !failures.is_empty() {
        r.outcome = if succeeded > 0 { "partial" } else { "error" }.into();
        r.errors = failures;
    }
    r
}
#[cfg(windows)]
pub fn start<R: tauri::Runtime>(
    app: AppHandle<R>,
    path: std::path::PathBuf,
) -> Result<(), &'static str> {
    // 先同步绑定首个 pipe，管道抢占会阻止 host 进入可用状态。
    let runtime = tauri::async_runtime::handle();
    let first = {
        // setup 在主线程执行；先进入持久的 host reactor，再同步绑定首个 pipe。
        let _entered = runtime.inner().enter();
        stillenough_core::transport::create_server(&path, true)?
    };
    tauri::async_runtime::spawn(async move {
        use stillenough_core::{transport, windows};
        let limits = Arc::new(tokio::sync::Semaphore::new(12));
        let mut listener = first;
        loop {
            if listener.connect().await.is_err() {
                return;
            }
            let Ok(next) = transport::create_server(&path, false) else {
                return;
            };
            let pipe = std::mem::replace(&mut listener, next);
            let Ok(permit) = limits.clone().try_acquire_owned() else {
                drop(pipe);
                continue;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let mut pipe = pipe;
                let exchange = async {
                    windows::verify_peer(&pipe, false)?;
                    let bytes = transport::read_frame(&mut pipe, transport::MAX_REQUEST).await?;
                    let request: Request =
                        serde_json::from_slice(&bytes).map_err(|_| "invalid_arguments")?;
                    let deadline = chrono::DateTime::parse_from_rfc3339(&request.deadline)
                        .map_err(|_| "invalid_arguments")?
                        .signed_duration_since(chrono::Utc::now())
                        .to_std()
                        .unwrap_or_default()
                        .min(std::time::Duration::from_secs(300));
                    let mut response = tokio::time::timeout(
                        deadline + std::time::Duration::from_millis(50),
                        handle(app.clone(), request.clone()),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        let mut r = denied(&app, &request, "deadline_exceeded");
                        if request.refreshes() {
                            r.data.operation_id = Some(request.request_id.clone());
                            r.data.operation_state = Some("running".into());
                        }
                        r
                    });
                    if response.errors.iter().all(|e| e.code != "version_mismatch") {
                        if let Err(code) = grant(&app, request.refreshes()) {
                            response = denied(&app, &request, code);
                        }
                    }
                    let reply = transport::Reply {
                        protocol_major: 1,
                        protocol_minor: 0,
                        schema_version: 1,
                        app_version: env!("CARGO_PKG_VERSION").into(),
                        instance_id: app.state::<AgentRuntime>().instance.clone(),
                        response,
                    };
                    transport::write_frame(&mut pipe, &reply, transport::MAX_RESPONSE).await?;
                    // 等客户端读完一帧再关闭，避免 Windows 丢弃未读 pipe 缓冲。
                    use tokio::io::AsyncReadExt;
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), pipe.read_u8())
                        .await;
                    Ok::<(), &'static str>(())
                };
                let _ = tokio::time::timeout(std::time::Duration::from_secs(302), exchange).await;
            });
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Databases;
    use rusqlite::Connection;
    use std::sync::atomic::{AtomicUsize, Ordering};
    pub(super) fn app() -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_app();
        let db = Connection::open_in_memory().unwrap();
        for sql in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_quota_only.sql"),
            include_str!("../migrations/003_connections.sql"),
            include_str!("../migrations/004_cli_authorization.sql"),
            include_str!("../migrations/005_monitoring.sql"),
            include_str!("../migrations/006_network.sql"),
        ] {
            db.execute_batch(sql).unwrap();
        }
        app.manage(AppState {
            databases: Mutex::new(Databases { production: db }),
            settings: Mutex::new(Default::default()),
            runtime: connections::Runtime::new().unwrap(),
            revision: std::sync::atomic::AtomicU64::new(0),
        });
        app.manage(AgentRuntime::default());
        app
    }
    pub(super) fn allow(app: &tauri::App<tauri::test::MockRuntime>, enabled: bool) {
        let state = app.state::<AppState>();
        access::save(
            &state.databases.lock().unwrap().production,
            &AgentAccess {
                version: 1,
                enabled,
                allow_refresh: enabled,
                allow_offline_cache: enabled,
                updated_at: Some(
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
                ),
            },
        )
        .unwrap();
    }
    pub(super) fn seed(app: &tauri::App<tauri::test::MockRuntime>, count: usize) {
        let state = app.state::<AppState>();
        let db = state.databases.lock().unwrap();
        for i in 0..count {
            let id = format!("a{i}");
            db.production.execute("INSERT INTO accounts VALUES(?1,'deepseek_api','合成别名','DeepSeek API',1,'now','now')",[&id]).unwrap();
            db.production.execute("INSERT INTO connections(account_id,base_url,credential_ref,revision) VALUES(?1,'https://api.deepseek.com','SECRET_REF','r')",[&id]).unwrap();
            db.production
                .execute(
                    "INSERT INTO fetch_states(account_id,status) VALUES(?1,'idle')",
                    [&id],
                )
                .unwrap();
        }
    }
    fn metric() -> crate::domain::models::QuotaMetric {
        crate::domain::models::QuotaMetric {
            metric_id: "wallet".into(),
            label: "余额".into(),
            kind: "balance".into(),
            unit: "CNY".into(),
            remaining: Some("1.234567890123456789".into()),
            limit: None,
            remaining_percent: None,
            resets_at: None,
            source: "official_api".into(),
            observed_at: chrono::Utc::now().to_rfc3339(),
            stale_after_seconds: 900,
        }
    }
    #[tokio::test]
    async fn permissions_protocol_and_snapshots_are_explicit() {
        let app = app();
        let req = Request::new(Command::Quota, 30);
        assert_eq!(
            handle(app.handle().clone(), req.clone()).await.errors[0].code,
            "agent_access_denied"
        );
        allow(&app, true);
        seed(&app, 1);
        let r = handle(app.handle().clone(), req).await;
        assert_eq!(r.transport, "ipc");
        assert_eq!(r.data.accounts.len(), 1);
        assert_eq!(
            r.instance_id.as_deref(),
            Some(app.state::<AgentRuntime>().instance.as_str())
        );
        assert!(!serde_json::to_string(&r).unwrap().contains("SECRET"));
        let mut req = Request::new(Command::Quota, 30);
        req.protocol_major = 99;
        assert_eq!(
            handle(app.handle().clone(), req).await.errors[0].code,
            "version_mismatch"
        );
    }
    #[tokio::test]
    async fn refresh_is_persisted_deduplicated_and_throttled() {
        let app = app();
        allow(&app, true);
        seed(&app, 1);
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        *app.state::<AppState>().runtime.test_fetch.lock().unwrap() = Some(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(vec![metric()]) })
        }));
        let mut req = Request::new(Command::Refresh, 30);
        req.account = Some("a0".into());
        let r = handle(app.handle().clone(), req.clone()).await;
        assert_eq!(r.outcome, "ok");
        assert_eq!(
            r.data.accounts[0].metrics[0].remaining.as_deref(),
            Some("1.234567890123456789")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let again = handle(app.handle().clone(), req).await;
        assert_eq!(again.outcome, "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let mut op = Request::new(Command::Operation, 30);
        op.operation_id = r.data.operation_id;
        assert_eq!(
            handle(app.handle().clone(), op)
                .await
                .data
                .operation_state
                .as_deref(),
            Some("completed")
        );
        let mut req = Request::new(Command::Refresh, 30);
        req.account = Some("a0".into());
        let r = handle(app.handle().clone(), req).await;
        assert_eq!(r.errors[0].code, "refresh_cooldown");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn global_concurrency_and_partial_failures_share_runtime() {
        let app = app();
        allow(&app, true);
        seed(&app, 6);
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let sequence = Arc::new(AtomicUsize::new(0));
        let (a, m, s) = (active.clone(), maximum.clone(), sequence.clone());
        *app.state::<AppState>().runtime.test_fetch.lock().unwrap() = Some(Arc::new(move || {
            let (a, m) = (a.clone(), m.clone());
            let n = s.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let current = a.fetch_add(1, Ordering::SeqCst) + 1;
                m.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                a.fetch_sub(1, Ordering::SeqCst);
                if n == 0 {
                    Err(crate::providers::FetchError::new("schema_changed"))
                } else {
                    Ok(vec![metric()])
                }
            })
        }));
        let mut req = Request::new(Command::Refresh, 30);
        req.all = true;
        let r = handle(app.handle().clone(), req).await;
        assert_eq!(r.outcome, "partial");
        assert_eq!(errors::exit_code(&r), 8);
        assert_eq!(maximum.load(Ordering::SeqCst), 3);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn revocation_cancels_queued_requests_and_hides_inflight_results() {
        let app = app();
        allow(&app, true);
        seed(&app, 4);
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let release = Arc::new(tokio::sync::Notify::new());
        let release_fake = release.clone();
        *app.state::<AppState>().runtime.test_fetch.lock().unwrap() = Some(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            let release = release_fake.clone();
            Box::pin(async move {
                release.notified().await;
                Ok(vec![metric()])
            })
        }));
        let mut req = Request::new(Command::Refresh, 30);
        req.all = true;
        let handle_app = app.handle().clone();
        let task = tokio::spawn(async move { handle(handle_app, req).await });
        for _ in 0..100 {
            if calls.load(Ordering::SeqCst) == 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        allow(&app, false);
        release.notify_waiters();
        let r = task.await.unwrap();
        assert_eq!(r.errors[0].code, "agent_access_denied");
        assert!(r.data.accounts.is_empty());
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
    #[tokio::test]
    async fn deadline_leaves_queryable_operation_and_restart_loses_it() {
        let app = app();
        allow(&app, true);
        seed(&app, 1);
        *app.state::<AppState>().runtime.test_fetch.lock().unwrap() = Some(Arc::new(|| {
            Box::pin(async {
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                Ok(vec![metric()])
            })
        }));
        let mut req = Request::new(Command::Refresh, 1);
        req.account = Some("a0".into());
        req.deadline = (chrono::Utc::now() + chrono::Duration::milliseconds(40)).to_rfc3339();
        let r = handle(app.handle().clone(), req).await;
        assert_eq!(errors::exit_code(&r), 7);
        let mut op = Request::new(Command::Operation, 30);
        op.operation_id = r.data.operation_id;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(
            handle(app.handle().clone(), op.clone())
                .await
                .data
                .operation_state
                .as_deref(),
            Some("completed")
        );
        app.state::<AgentRuntime>()
            .operations
            .lock()
            .unwrap()
            .clear();
        assert_eq!(
            handle(app.handle().clone(), op).await.errors[0].code,
            "operation_lost"
        );
    }
    #[tokio::test]
    async fn live_doctor_retries_a_previous_auth_failure() {
        let app = app();
        allow(&app, true);
        seed(&app, 1);
        app.state::<AppState>()
            .databases
            .lock()
            .unwrap()
            .production
            .execute(
                "UPDATE fetch_states SET status='auth_required',error_code='auth_required'",
                [],
            )
            .unwrap();
        *app.state::<AppState>().runtime.test_fetch.lock().unwrap() =
            Some(Arc::new(|| Box::pin(async { Ok(vec![metric()]) })));
        let mut req = Request::new(Command::Doctor, 30);
        req.account = Some("a0".into());
        req.live = true;
        let r = handle(app.handle().clone(), req).await;
        assert_eq!(r.outcome, "ok");
        assert!(r
            .data
            .checks
            .iter()
            .any(|c| c.name == "live-query" && c.state == "ok"));
    }
}

#[cfg(all(test, windows))]
mod pipe_tests {
    use super::*;
    #[test]
    fn setup_binds_pipe_without_an_ambient_tokio_runtime() {
        let app = super::tests::app();
        super::tests::allow(&app, true);
        let path = std::path::PathBuf::from(format!("agent-setup-test-{}", uuid::Uuid::new_v4()));
        start(app.handle().clone(), path.clone()).unwrap();
        let r = tauri::async_runtime::block_on(stillenough_core::transport::query(
            &path,
            &Request::new(Command::Quota, 5),
        ))
        .unwrap();
        assert_eq!(r.outcome, "ok");
    }

    #[tokio::test]
    async fn windows_server_shares_authorized_snapshots_and_revocation() {
        let app = super::tests::app();
        super::tests::allow(&app, true);
        super::tests::seed(&app, 1);
        let path = std::path::PathBuf::from(format!("agent-pipe-test-{}", uuid::Uuid::new_v4()));
        start(app.handle().clone(), path.clone()).unwrap();
        let mut jobs = Vec::new();
        for _ in 0..4 {
            let path = path.clone();
            jobs.push(tokio::spawn(async move {
                stillenough_core::transport::query(&path, &Request::new(Command::Quota, 5))
                    .await
                    .unwrap()
            }));
        }
        for job in jobs {
            let r = job.await.unwrap();
            assert_eq!(r.outcome, "ok");
            assert_eq!(r.data.accounts.len(), 1);
            assert_eq!(
                r.instance_id.as_deref(),
                Some(app.state::<AgentRuntime>().instance.as_str())
            );
        }
        super::tests::allow(&app, false);
        let r = stillenough_core::transport::query(&path, &Request::new(Command::Quota, 5))
            .await
            .unwrap();
        assert_eq!(r.errors[0].code, "agent_access_denied");
        assert!(r.data.accounts.is_empty());
    }
}
