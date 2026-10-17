mod commands;
mod domain;
mod infra;
mod providers;
mod services;

use commands::{AppState, Databases};
use infra::sqlite::open_database;
use std::sync::{Mutex, OnceLock};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

/// The tray's pause item lives here rather than in `AppState`: it is a handle to
/// a native menu object, and keeping native handles out of the shared state also
/// keeps the menu code out of the unit-test binary, which has no SxS manifest
/// and therefore cannot resolve comctl32 v6's `TaskDialogIndirect`.
static TRAY_PAUSE: OnceLock<Mutex<Option<MenuItem<tauri::Wry>>>> = OnceLock::new();
static TRAY_WIDGET: OnceLock<Mutex<Option<MenuItem<tauri::Wry>>>> = OnceLock::new();

pub(crate) fn set_tray_widget_label(visible: bool) {
    if let Some(slot) = TRAY_WIDGET.get() {
        if let Ok(item) = slot.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(if visible {
                    "隐藏浮窗"
                } else {
                    "显示浮窗"
                });
            }
        }
    }
}

fn widget_is_visible(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("widget")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// Points the tray label at the action the menu will actually perform next.
pub(crate) fn set_tray_pause_label(paused: bool) {
    if let Some(slot) = TRAY_PAUSE.get() {
        if let Ok(item) = slot.lock() {
            if let Some(item) = item.as_ref() {
                let _ = item.set_text(if paused {
                    "恢复额度采集"
                } else {
                    "暂停额度采集"
                });
            }
        }
    }
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开 StillEnough", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "刷新额度", true, None::<&str>)?;
    let widget = MenuItem::with_id(app, "widget", "显示浮窗", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let pause_label = if app
        .state::<AppState>()
        .settings
        .lock()
        .map(|settings| settings.collection_paused)
        .unwrap_or(false)
    {
        "恢复额度采集"
    } else {
        "暂停额度采集"
    };
    let pause = MenuItem::with_id(app, "pause", pause_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open, &refresh, &widget, &separator, &pause, &separator, &quit,
        ],
    )?;
    let mut builder = TrayIconBuilder::new()
        .tooltip("StillEnough · AI 额度与余额监控")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            // Driven from the backend so it also works while both windows are hidden.
            "refresh" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    services::connections::refresh_all(handle, true).await;
                });
            }
            "pause" => {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                let paused = state
                    .settings
                    .lock()
                    .map(|settings| settings.collection_paused)
                    .unwrap_or(false);
                // The command owns the tray label, so every caller stays in sync.
                let _ = commands::set_collection_paused(!paused, state, app.clone());
            }
            "widget" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = handle.try_state::<AppState>() {
                        let visible = !widget_is_visible(&handle);
                        let _ = commands::set_widget_visible(visible, handle.clone(), state).await;
                    }
                });
            }
            "quit" => app.exit(0),
            _ => (),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    if let Ok(mut slot) = TRAY_PAUSE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(pause);
    }
    if let Ok(mut slot) = TRAY_WIDGET.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(widget);
    }
    set_tray_widget_label(widget_is_visible(app.handle()));
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_menu_event(|app, event| {
            if let Some(action) = event.id().as_ref().strip_prefix("stillenough-widget-") {
                if ["open", "refresh", "collapse", "pin", "hide", "quit"].contains(&action) {
                    let _ = app.emit_to("widget", "widget-menu-action", action);
                }
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let production = open_database(&data_dir.join("ai-plan-manager.sqlite"))
                .map_err(std::io::Error::other)?;
            let settings = commands::load_settings(&production);
            let runtime = services::connections::Runtime::new().map_err(std::io::Error::other)?;
            runtime.network.load(&production);
            app.manage(AppState {
                databases: Mutex::new(Databases { production }),
                settings: Mutex::new(settings),
                runtime,
                revision: std::sync::atomic::AtomicU64::new(0),
            });
            services::connections::start_scheduler(app.handle().clone());
            commands::restore_widget_on_launch(app.handle());
            create_tray(app)?;
            if let Some(window) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(window) = handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::list_provider_descriptors,
            commands::begin_connection_setup,
            commands::set_setup_credential,
            commands::probe_connection,
            commands::cancel_connection_setup,
            commands::save_account_with_connection,
            commands::remove_account,
            commands::get_settings,
            commands::get_network_settings,
            commands::save_network_settings,
            commands::update_refresh_interval,
            commands::set_collection_paused,
            commands::state_revision,
            commands::set_autostart,
            commands::set_theme,
            commands::sync_native_theme,
            commands::set_widget_options,
            commands::set_widget_visible,
            commands::open_main_window,
            commands::quit_app,
            commands::refresh_account,
            commands::list_alert_events,
            commands::unread_alert_count,
            commands::mark_alert_read,
            commands::mark_all_alerts_read,
            commands::get_history,
            commands::update_monitoring_settings,
            commands::export_data,
            commands::clear_all_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running StillEnough");
}
