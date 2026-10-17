use crate::{
    domain::models::{
        Account, AccountInput, AlertEvent, AppSettings, ExportSummary, HistorySeries,
        MonitoringSettings,
    },
    providers::registry,
    services::{accounts, alerts, connections, export, history},
};
use rusqlite::Connection;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, MutexGuard,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub struct Databases {
    pub production: Connection,
}
pub struct AppState {
    pub databases: Mutex<Databases>,
    pub settings: Mutex<AppSettings>,
    pub runtime: connections::Runtime,
    /// Monotonic version of everything the windows render. A read that started
    /// under an older revision must not be painted over a newer one.
    pub revision: AtomicU64,
}

pub fn revision(state: &AppState) -> u64 {
    state.revision.load(Ordering::Acquire)
}

/// Bumps the revision and tells both windows the state moved on.
pub fn publish(app: &AppHandle, state: &AppState) -> u64 {
    let next = state.revision.fetch_add(1, Ordering::AcqRel) + 1;
    let _ = app.emit("state-changed", next);
    // Older builds listen for this name; keeping it costs nothing and lets a
    // window that has not yet adopted revisions still reload.
    let _ = app.emit("dashboard-changed", ());
    next
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    mutex
        .lock()
        .map_err(|_| "本地状态锁不可用，请重启应用".into())
}

fn persist_settings(state: &AppState, settings: &AppSettings) -> Result<(), String> {
    let value = serde_json::to_string(settings).map_err(|error| error.to_string())?;
    let databases = lock(&state.databases)?;
    databases.production.execute(
        "INSERT INTO app_settings(key,value_json,updated_at) VALUES('app',?1,?2) \
         ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
        rusqlite::params![value, chrono::Utc::now().to_rfc3339()],
    ).map_err(|error| format!("保存设置失败：{error}"))?;
    Ok(())
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, String> {
    let seconds = (u64::from(lock(&state.settings)?.refresh_minutes) * 120).max(900);
    let databases = lock(&state.databases)?;
    let mut accounts = accounts::list(&databases.production)?;
    for account in &mut accounts {
        account.refreshing = lock(&state.runtime.gates)?
            .get(&account.id)
            .is_some_and(|gate| gate.busy);
        for metric in &mut account.metrics {
            metric.stale_after_seconds = seconds;
        }
    }
    Ok(accounts)
}

fn require_main(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("此操作仅允许管理窗口执行".into())
    }
}
#[tauri::command]
pub fn list_provider_descriptors() -> Vec<crate::domain::models::ProviderDescriptor> {
    registry::descriptors()
}
#[tauri::command]
pub async fn begin_connection_setup(
    input: AccountInput,
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<String, String> {
    require_main(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        connections::begin(&state, input)
    })
    .await
    .map_err(|_| "后台任务失败".to_string())?
}
#[tauri::command]
pub fn set_setup_credential(
    session_id: String,
    key: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let key = zeroize::Zeroizing::new(key);
    require_main(&window)?;
    connections::set_key(&state, &session_id, key)
}
#[tauri::command]
pub async fn probe_connection(
    session_id: String,
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<crate::domain::models::ProbeResult, String> {
    require_main(&window)?;
    connections::probe(app, session_id).await
}
#[tauri::command]
pub fn cancel_connection_setup(
    session_id: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    require_main(&window)?;
    connections::cancel(&state, &session_id);
    Ok(())
}
#[tauri::command]
pub async fn save_account_with_connection(
    session_id: String,
    alias: String,
    enabled: bool,
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<Account, String> {
    require_main(&window)?;
    let handle = app.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        {
            let mut sessions = lock(&state.runtime.sessions)?;
            let setup = sessions.get_mut(&session_id).ok_or("配置已过期")?;
            setup.input.alias = alias;
            setup.input.enabled = enabled;
            crate::domain::validation::validate_account(&setup.input)?;
        }
        let result = connections::save(&state, &session_id);
        connections::cleanup(&state);
        result
    })
    .await
    .map_err(|_| "后台任务失败".to_string())??;
    let state = app.state::<AppState>();
    publish(&app, &state);
    Ok(saved)
}
#[tauri::command]
pub async fn remove_account(
    id: String,
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<bool, String> {
    require_main(&window)?;
    let handle = app.clone();
    let removed = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let removed = connections::remove(&state, &id)?;
        connections::cleanup(&state);
        let pending: i64 = lock(&state.databases)?
            .production
            .query_row("SELECT COUNT(*) FROM credential_cleanup", [], |r| r.get(0))
            .map_err(|_| "凭据清理状态不可用")?;
        if pending > 0 {
            publish(&handle, &state);
            return Err("账号已删除，系统凭据清理待后台重试".into());
        }
        Ok::<_, String>(removed)
    })
    .await
    .map_err(|_| "后台任务失败".to_string())??;
    let state = app.state::<AppState>();
    publish(&app, &state);
    Ok(removed)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>, app: AppHandle) -> Result<AppSettings, String> {
    let mut current = lock(&state.settings)?;
    let resolved = current.language.resolve();
    let changed = current.resolved_language != resolved;
    current.resolved_language = resolved.into();
    let settings = current.clone();
    drop(current);
    if changed {
        crate::update_tray_language(&app, &settings);
        publish(&app, &state);
    }
    Ok(settings)
}

pub fn load_settings(connection: &rusqlite::Connection) -> AppSettings {
    let stored = connection.query_row(
        "SELECT value_json FROM app_settings WHERE key='app'",
        [],
        |row| row.get::<_, String>(0),
    );
    stored
        .ok()
        .and_then(|json| serde_json::from_str::<AppSettings>(&json).ok())
        .unwrap_or_default()
        .sanitize()
}

#[tauri::command]
pub fn update_refresh_interval(
    minutes: u8,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppSettings, String> {
    if ![5, 10, 15, 30].contains(&minutes) {
        return Err("刷新间隔仅支持 5、10、15 或 30 分钟".into());
    }
    let mut settings = lock(&state.settings)?;
    let mut next = settings.clone();
    next.refresh_minutes = minutes;
    persist_settings(&state, &next)?;
    *settings = next.clone();
    drop(settings);
    publish(&app, &state);
    Ok(next)
}

/// Stops or resumes every scheduled query. Account-level `enabled` is untouched,
/// so resuming restores exactly the set that was running before.
#[tauri::command]
pub fn set_collection_paused(
    paused: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppSettings, String> {
    let mut current = lock(&state.settings)?;
    if current.collection_paused == paused {
        return Ok(current.clone());
    }
    let mut next = current.clone();
    next.collection_paused = paused;
    persist_settings(&state, &next)?;
    *current = next.clone();
    drop(current);
    crate::set_tray_pause_label(paused);
    publish(&app, &state);
    if !paused {
        // Resuming should show current numbers, not wait out the next tick.
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            connections::refresh_all(handle, true).await;
        });
    }
    Ok(next)
}

#[tauri::command]
pub fn state_revision(state: State<'_, AppState>) -> u64 {
    revision(&state)
}

#[tauri::command]
pub fn set_autostart(
    enabled: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppSettings, String> {
    let effective = crate::infra::autostart::apply(enabled)?;
    let mut current = lock(&state.settings)?;
    let mut next = current.clone();
    next.autostart = effective;
    persist_settings(&state, &next)?;
    *current = next.clone();
    drop(current);
    publish(&app, &state);
    Ok(next)
}

/// Applies every widget preference in one write so the window never renders a
/// half-applied mix of the previous and the new options.
#[tauri::command]
pub fn set_theme(
    theme: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppSettings, String> {
    let mode = crate::domain::models::ThemeMode::parse(&theme)?;
    let mut current = lock(&state.settings)?;
    let mut next = current.clone();
    next.theme = mode;
    persist_settings(&state, &next)?;
    *current = next.clone();
    drop(current);
    publish(&app, &state);
    Ok(next)
}

#[tauri::command]
pub fn set_language(
    language: String,
    state: State<'_, AppState>,
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<AppSettings, String> {
    require_main(&window)?;
    let preference = crate::domain::models::LanguagePreference::parse(&language)?;
    let mut current = lock(&state.settings)?;
    let mut next = current.clone();
    next.language = preference;
    next.resolved_language = preference.resolve().into();
    persist_settings(&state, &next)?;
    *current = next.clone();
    drop(current);
    crate::update_tray_language(&app, &next);
    publish(&app, &state);
    Ok(next)
}

/// Keeps the native title bar and window chrome in step with the web content.
///
/// The webview is what actually resolves `prefers-color-scheme`, so it reports
/// the side it settled on rather than the backend guessing it a second way.
#[tauri::command]
pub fn sync_native_theme(app: AppHandle, dark: bool) {
    let native = if dark {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    for (_, window) in app.webview_windows() {
        let _ = window.set_theme(Some(native));
    }
}

#[tauri::command]
pub fn set_widget_options(
    options: crate::domain::models::WidgetOptions,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppSettings, String> {
    let opacity = options.opacity.clamp(
        crate::domain::models::MIN_WIDGET_OPACITY,
        crate::domain::models::MAX_WIDGET_OPACITY,
    );
    let mut current = lock(&state.settings)?;
    let mut next = current.clone();
    if let Some(mut display) = options.display {
        display.max_accounts = display.max_accounts.clamp(1, 8);
        next.widget_display = display;
    }
    next.widget_collapsed = options.collapsed;
    next.widget_pinned = options.pinned;
    next.widget_opacity = opacity;
    next.widget_accounts = options.accounts.map(|accounts| {
        let mut seen = std::collections::BTreeSet::new();
        accounts
            .into_iter()
            .filter(|id| !id.trim().is_empty() && seen.insert(id.clone()))
            .collect::<Vec<_>>()
    });
    if next
        .widget_accounts
        .as_ref()
        .is_some_and(|list| list.is_empty())
    {
        next.widget_accounts = None;
    }
    persist_settings(&state, &next)?;
    *current = next.clone();
    drop(current);
    if let Some(window) = app.get_webview_window("widget") {
        // Only the parts the backend owns; opacity and collapse are the widget's own render.
        let _ = window.set_always_on_top(options.pinned);
    }
    publish(&app, &state);
    Ok(next)
}

/// Creates the widget window hidden, with the drag-to-persist handler attached.
///
/// Building it hidden and letting the caller show it avoids a visible flash at
/// the default position before the saved one is applied.
fn build_widget_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let window = WebviewWindowBuilder::new(
        app,
        "widget",
        WebviewUrl::App("index.html?view=widget".into()),
    )
    .title("额度浮窗")
    .inner_size(280.0, 185.0)
    .min_inner_size(280.0, 80.0)
    .always_on_top(true)
    .skip_taskbar(true)
    .decorations(false)
    .shadow(false)
    .visible(false)
    .transparent(true)
    .resizable(false)
    .build()
    .map_err(|e| e.to_string())?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(position) = event {
            let position = (position.x, position.y);
            if position == (-32000, -32000) {
                return;
            }
            let state = handle.state::<AppState>();
            if let Ok(mut settings) = state.settings.lock() {
                settings.widget_position = Some(position);
            }
            let handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    let state = handle.state::<AppState>();
                    if let Ok(settings) = state.settings.lock() {
                        if settings.widget_position == Some(position) {
                            let _ = persist_settings(&state, &settings);
                        }
                    };
                })
                .await;
            });
        }
    });
    Ok(window)
}

/// Whether the widget is on screen right now.
///
/// A stored preference that says "visible" while no window exists is what made
/// the first click after a restart do nothing, so the truth is always derived
/// from the window rather than from what the caller asked for.
fn resolve_widget_visibility(window_exists: bool, requested: bool) -> bool {
    window_exists && requested
}

/// Brings the widget to the requested state and reports what actually happened.
///
/// Asking to hide a window that was never built is not an error; it simply
/// reports that nothing is on screen.
pub fn apply_widget_visibility(
    app: &AppHandle,
    state: &AppState,
    visible: bool,
) -> Result<bool, String> {
    match app.get_webview_window("widget") {
        Some(window) => {
            if visible {
                window.show().map_err(|e| e.to_string())?;
                restore_widget_position(&window, state)?;
            } else {
                window.hide().map_err(|e| e.to_string())?;
            }
        }
        None if visible => {
            let window = build_widget_window(app)?;
            restore_widget_position(&window, state)?;
            window.show().map_err(|e| e.to_string())?;
        }
        None => return Ok(false),
    }
    Ok(app
        .get_webview_window("widget")
        .and_then(|window| window.is_visible().ok())
        .map(|shown| resolve_widget_visibility(true, shown))
        .unwrap_or(false))
}

fn record_widget_visibility(app: &AppHandle, state: &AppState, actual: bool) -> Result<(), String> {
    // Native visibility already changed, even if saving the preference fails.
    crate::set_tray_widget_label(actual);
    let mut settings = lock(&state.settings)?;
    if settings.widget_visible == actual {
        return Ok(());
    }
    let mut next = settings.clone();
    next.widget_visible = actual;
    persist_settings(state, &next)?;
    *settings = next;
    drop(settings);
    publish(app, state);
    Ok(())
}

#[tauri::command]
pub async fn set_widget_visible(
    visible: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let actual = apply_widget_visibility(&app, &state, visible)?;
    record_widget_visibility(&app, &state, actual)?;
    Ok(actual)
}

/// Reopens the widget the user left on, so the stored preference is true again
/// from the first frame instead of describing a window that no longer exists.
pub fn restore_widget_on_launch(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state
        .settings
        .lock()
        .map(|settings| settings.widget_visible)
        .unwrap_or(false)
    {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Off the setup thread: building a webview during setup can deadlock.
        let actual =
            apply_widget_visibility(&handle, &handle.state::<AppState>(), true).unwrap_or(false);
        if let Err(error) = record_widget_visibility(&handle, &handle.state::<AppState>(), actual) {
            eprintln!("浮窗恢复失败：{error}");
        }
    });
}

fn clamp_widget_position(
    position: (i32, i32),
    size: (u32, u32),
    area: (i32, i32, u32, u32),
) -> (i32, i32) {
    let (x, y, width, height) = area;
    let max_x = i64::from(x) + i64::from(width.saturating_sub(size.0));
    let max_y = i64::from(y) + i64::from(height.saturating_sub(size.1));
    (
        i64::from(position.0).clamp(i64::from(x), max_x) as i32,
        i64::from(position.1).clamp(i64::from(y), max_y) as i32,
    )
}

fn restore_widget_position(window: &tauri::WebviewWindow, state: &AppState) -> Result<(), String> {
    let saved = lock(&state.settings)?.widget_position;
    let Some(position) = saved else {
        return Ok(());
    };
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let area = monitor.work_area();
            i64::from(position.0) >= i64::from(area.position.x)
                && i64::from(position.1) >= i64::from(area.position.y)
                && i64::from(position.0) < i64::from(area.position.x) + i64::from(area.size.width)
                && i64::from(position.1) < i64::from(area.position.y) + i64::from(area.size.height)
        })
        .cloned()
        .or(window.primary_monitor().map_err(|e| e.to_string())?)
        .or_else(|| monitors.first().cloned());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let size = window.outer_size().map_err(|e| e.to_string())?;
        let (x, y) = clamp_widget_position(
            position,
            (size.width, size.height),
            (
                area.position.x,
                area.position.y,
                area.size.width,
                area.size.height,
            ),
        );
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_main_window(app: AppHandle) {
    crate::show_main(&app);
}

/// Exits the whole app, matching what the tray's 退出 item does.
///
/// Settings and observations are written as they change, so there is nothing
/// to flush here and no per-account work to stop.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn refresh_account(id: String, app: AppHandle) -> Result<(), String> {
    connections::refresh(app, id, true).await
}

#[tauri::command]
pub fn list_alert_events(
    account_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<AlertEvent>, String> {
    let databases = lock(&state.databases)?;
    alerts::list(&databases.production, account_id.as_deref(), 100)
}

#[tauri::command]
pub fn unread_alert_count(state: State<'_, AppState>) -> Result<i64, String> {
    let databases = lock(&state.databases)?;
    alerts::unread_count(&databases.production)
}

#[tauri::command]
pub fn mark_alert_read(id: i64, state: State<'_, AppState>, app: AppHandle) -> Result<i64, String> {
    let databases = lock(&state.databases)?;
    alerts::mark_read(&databases.production, id)?;
    let remaining = alerts::unread_count(&databases.production)?;
    drop(databases);
    let _ = app.emit("alerts-changed", ());
    Ok(remaining)
}

#[tauri::command]
pub fn mark_all_alerts_read(state: State<'_, AppState>, app: AppHandle) -> Result<i64, String> {
    let databases = lock(&state.databases)?;
    alerts::mark_all_read(&databases.production)?;
    drop(databases);
    let _ = app.emit("alerts-changed", ());
    Ok(0)
}

#[tauri::command]
pub fn get_history(
    days: u32,
    account_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<HistorySeries>, String> {
    if !matches!(days, 7 | 30) {
        return Err("历史范围仅支持 7 天或 30 天".into());
    }
    let minutes = u64::from(lock(&state.settings)?.refresh_minutes);
    let databases = lock(&state.databases)?;
    history::list(
        &databases.production,
        &history::Query {
            days,
            account_id,
            // A silent stretch longer than a few refresh intervals is an outage, not a line.
            gap_seconds: (minutes * 240).max(1800) as i64,
        },
    )
}

#[tauri::command]
pub fn update_monitoring_settings(
    settings: MonitoringSettings,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<MonitoringSettings, String> {
    if !history::ALLOWED_RETENTION_DAYS.contains(&settings.history_retention_days) {
        return Err("历史保留仅支持 30、60 或 90 天".into());
    }
    let next_settings = MonitoringSettings {
        alert_thresholds: alerts::sanitize_thresholds(&settings.alert_thresholds),
        ..settings
    };
    let mut current = lock(&state.settings)?;
    let mut next = current.clone();
    next.alert_enabled = next_settings.alert_enabled;
    next.alert_thresholds = next_settings.alert_thresholds.clone();
    next.history_retention_days = next_settings.history_retention_days;
    persist_settings(&state, &next)?;
    *current = next;
    drop(current);
    {
        let databases = lock(&state.databases)?;
        history::prune(&databases.production, next_settings.history_retention_days)?;
    }
    publish(&app, &state);
    Ok(next_settings)
}

#[tauri::command]
pub async fn export_data(
    format: String,
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<ExportSummary, String> {
    require_main(&window)?;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let rendered = {
            let databases = state.databases.lock().map_err(|_| "本地状态不可用")?;
            export::render(&databases.production, &format)?
        };
        let directory = app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("无法确定导出目录：{error}"))?
            .join("exports");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("无法创建导出目录：{error}"))?;
        export::write_file(&directory, &format, &rendered)
    })
    .await
    .map_err(|_| "后台任务失败".to_string())?
}

#[tauri::command]
pub async fn clear_all_data(
    confirm: String,
    window: tauri::WebviewWindow,
    app: AppHandle,
) -> Result<usize, String> {
    require_main(&window)?;
    if confirm != "删除全部数据" {
        return Err("请确认后清除全部本地数据".into());
    }
    let handle = app.clone();
    let removed = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let removed = {
            let mut databases = state.databases.lock().map_err(|_| "本地状态不可用")?;
            export::clear_all(&mut databases.production)?
        };
        // Queued credential references are only purged once nothing can use them again.
        connections::cleanup(&state);
        Ok::<usize, String>(removed)
    })
    .await
    .map_err(|_| "后台任务失败".to_string())??;
    let state = app.state::<AppState>();
    publish(&app, &state);
    let _ = app.emit("alerts-changed", ());
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_position_keeps_visible_coordinates_and_recovers_offscreen() {
        assert_eq!(
            clamp_widget_position((150, 200), (280, 185), (0, 0, 1920, 1040)),
            (150, 200)
        );
        assert_eq!(
            clamp_widget_position((1900, 1030), (280, 185), (0, 0, 1920, 1040)),
            (1640, 855)
        );
        assert_eq!(
            clamp_widget_position((-1600, 100), (280, 185), (-1920, 0, 1920, 1040)),
            (-1600, 100)
        );
        assert_eq!(
            clamp_widget_position((-1600, 100), (280, 185), (0, 0, 1920, 1040)),
            (0, 100)
        );
        assert_eq!(
            clamp_widget_position((50, 50), (560, 370), (0, 0, 320, 240)),
            (0, 0)
        );
    }

    #[test]
    fn widget_position_survives_settings_reload() {
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch("CREATE TABLE app_settings(key TEXT PRIMARY KEY, value_json TEXT);")
            .unwrap();
        database
            .execute(
                "INSERT INTO app_settings VALUES('app', ?1)",
                [r#"{"refreshMinutes":10,"widgetVisible":false,"widgetPosition":[-1500,120]}"#],
            )
            .unwrap();
        assert_eq!(load_settings(&database).widget_position, Some((-1500, 120)));
    }

    #[test]
    fn a_widget_that_was_never_built_is_never_reported_as_visible() {
        // Hiding something that does not exist is not an error, but the state
        // handed back must be the truth, or the next toggle starts from a lie
        // and the user's first click appears to do nothing.
        assert!(!resolve_widget_visibility(false, false));
        assert!(!resolve_widget_visibility(false, true));
        assert!(resolve_widget_visibility(true, true));
        assert!(!resolve_widget_visibility(true, false));
    }

    #[test]
    fn legacy_demo_setting_does_not_change_account_space_or_preferences() {
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch("CREATE TABLE app_settings(key TEXT PRIMARY KEY, value_json TEXT);")
            .unwrap();
        database
            .execute(
                "INSERT INTO app_settings VALUES('app', ?1)",
                [r#"{"demoMode":true,"refreshMinutes":15,"widgetVisible":true}"#],
            )
            .unwrap();
        let settings = load_settings(&database);
        assert_eq!(settings.refresh_minutes, 15);
        assert!(settings.widget_visible);
        let serialized = serde_json::to_value(settings).unwrap();
        assert!(serialized.get("demoMode").is_none());
    }

    fn settings_database(json: &str) -> Connection {
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch("CREATE TABLE app_settings(key TEXT PRIMARY KEY, value_json TEXT);")
            .unwrap();
        database
            .execute("INSERT INTO app_settings VALUES('app', ?1)", [json])
            .unwrap();
        database
    }

    #[test]
    fn stored_settings_from_an_older_build_get_safe_monitoring_defaults() {
        let database = settings_database(r#"{"refreshMinutes":10,"widgetVisible":false}"#);
        let settings = load_settings(&database);
        assert!(settings.alert_enabled);
        assert_eq!(
            settings.alert_thresholds,
            crate::services::alerts::DEFAULT_THRESHOLDS.to_vec()
        );
        assert_eq!(
            settings.history_retention_days,
            crate::services::history::DEFAULT_RETENTION_DAYS
        );
    }

    #[test]
    fn sqlite_language_reload_preserves_other_preferences_and_recomputes_runtime_language() {
        for language in ["zh-CN", "en", "system", "future"] {
            let json = format!(
                r#"{{"refreshMinutes":15,"widgetVisible":true,"language":"{language}","resolvedLanguage":"untrusted","widgetOpacity":75}}"#
            );
            let database = settings_database(&json);
            let settings = load_settings(&database);
            assert_eq!(settings.resolved_language, settings.language.resolve());
            assert_eq!(settings.refresh_minutes, 15);
            assert_eq!(settings.widget_opacity, 75);
            assert!(settings.widget_visible);
        }
    }

    #[test]
    fn nonsense_stored_monitoring_values_are_replaced_by_defaults() {
        let database = settings_database(
            r#"{"refreshMinutes":10,"alertThresholds":[-5,900],"historyRetentionDays":7}"#,
        );
        let settings = load_settings(&database);
        assert_eq!(
            settings.alert_thresholds,
            crate::services::alerts::DEFAULT_THRESHOLDS.to_vec()
        );
        assert_eq!(
            settings.history_retention_days,
            crate::services::history::DEFAULT_RETENTION_DAYS
        );
        assert_eq!(
            MonitoringSettings::from(&settings).alert_thresholds,
            settings.alert_thresholds
        );
    }

    #[test]
    fn settings_from_an_older_build_get_working_pause_widget_and_autostart_defaults() {
        let database = settings_database(r#"{"refreshMinutes":10,"widgetVisible":true}"#);
        let settings = load_settings(&database);
        assert!(!settings.collection_paused, "旧设置不应默认暂停采集");
        assert!(!settings.autostart, "旧设置不应凭空开启开机启动");
        assert_eq!(settings.widget_opacity, 100, "旧设置应显示完全不透明");
        assert!(!settings.widget_collapsed);
        assert!(settings.widget_pinned, "浮窗默认保持置顶");
        assert!(
            settings.widget_accounts.is_none(),
            "未指定关注账号时保持自动排序"
        );
    }

    #[test]
    fn widget_opacity_is_clamped_to_a_readable_range() {
        let clamped = |value: u8| {
            crate::domain::models::AppSettings {
                widget_opacity: value,
                ..Default::default()
            }
            .sanitize()
            .widget_opacity
        };
        assert_eq!(clamped(5), 50);
        assert_eq!(clamped(0), 50);
        assert_eq!(clamped(255), 100);
        assert_eq!(clamped(80), 80, "区间内的值必须原样保留");
    }

    #[test]
    fn a_blank_or_duplicate_widget_account_list_falls_back_to_automatic_order() {
        let settings = crate::domain::models::AppSettings {
            widget_accounts: Some(vec!["a".into(), "a".into(), "  ".into(), "b".into()]),
            ..Default::default()
        }
        .sanitize();
        assert_eq!(
            settings.widget_accounts,
            Some(vec!["a".to_string(), "b".to_string()]),
            "重复和空白条目必须清掉"
        );

        let empty = crate::domain::models::AppSettings {
            widget_accounts: Some(vec![" ".into()]),
            ..Default::default()
        }
        .sanitize();
        assert!(
            empty.widget_accounts.is_none(),
            "全部清空后应回到自动排序，而不是让浮窗空白"
        );
    }

    #[test]
    fn every_mutation_advances_the_revision_so_windows_can_reject_stale_reads() {
        let state = AppState {
            databases: Mutex::new(Databases {
                production: Connection::open_in_memory().unwrap(),
            }),
            settings: Mutex::new(crate::domain::models::AppSettings::default()),
            runtime: connections::Runtime::new().unwrap(),
            revision: std::sync::atomic::AtomicU64::new(0),
        };
        assert_eq!(revision(&state), 0);
        state
            .revision
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        assert_eq!(revision(&state), 1);
    }
}

#[tauri::command]
pub fn get_network_settings(
    state: State<'_, AppState>,
) -> Result<crate::services::network::NetworkConfig, String> {
    state.runtime.network.config()
}
#[tauri::command]
pub fn save_network_settings(
    config: crate::services::network::NetworkConfig,
    state: State<'_, AppState>,
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("请在主窗口修改查询网络设置".into());
    }
    let db = lock(&state.databases)?;
    state.runtime.network.save(&db.production, config)?;
    drop(db);
    publish(&app, &state);
    Ok(())
}
