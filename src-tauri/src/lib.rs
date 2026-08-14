mod commands;
pub mod discovery;
pub mod error;
pub mod server;
pub mod storage;
pub mod transfer;

use server::{SharedState, WsEvent};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use storage::Database;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tokio::sync::Mutex;

const APP_DATA_DIR_NAME: &str = "LanNook";
const LEGACY_APP_DATA_DIR_NAME: &str = "LYNQO";
const DATABASE_FILE_NAME: &str = "lannook.db";
const LEGACY_DATABASE_FILE_NAME: &str = "lynqo.db";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Migrate before SQLite or the log appender opens any file. The old
    // application identifier remains for updater compatibility, but user data
    // lives under the new product name from this release onward.
    let _ = get_app_data_dir();

    // Keep the non-blocking guard alive for the entire application lifetime;
    // otherwise buffered log writes can be dropped when the function returns.
    let log_dir = get_log_dir();
    let file_appender = tracing_appender::rolling::daily(&log_dir, "lannook.log");
    let (non_blocking, _file_log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(non_blocking)
        .init();

    // Open database in app data directory. A failure here (corrupt file,
    // missing permissions, disk error) aborts startup with the concrete path
    // and reason in the log instead of a bare panic.
    let db_path = get_db_path();
    let db = match Database::open(&db_path) {
        Ok(db) => db,
        Err(error) => {
            tracing::error!(
                "Failed to open database at {}: {}",
                db_path.display(),
                error
            );
            panic!(
                "Failed to open database at {}: {}",
                db_path.display(),
                error
            );
        }
    };
    let db = Arc::new(db);

    // Opportunistically purge transfer history older than 30 days so the
    // database does not grow without bound. Files on disk are untouched.
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(i64::MAX);
        if let Err(error) = db.purge_old_transfers(now - 30 * 24 * 3600) {
            tracing::warn!("Failed to purge old transfer history: {}", error);
        }
    }

    // Create shared application state
    let state: SharedState = Arc::new(Mutex::new(server::AppState::new(db)));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus existing window when second instance launched
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state.clone())
        .setup(move |app| {
            // Network adapters can change while the app is hidden in the tray.
            // Keep the backend endpoint and mDNS record current even when no
            // connection panel is open to trigger an IPC refresh.
            let monitor_state = state.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    commands::server_cmd::synchronize_network_state(&monitor_state).await;
                }
            });

            // Build tray menu
            let show_item = MenuItem::with_id(app, "show", "打开 LanNook", true, None::<&str>)?;
            let start_item =
                MenuItem::with_id(app, "start_service", "开始局域网服务", true, None::<&str>)?;
            let stop_item =
                MenuItem::with_id(app, "stop_service", "停止局域网服务", true, None::<&str>)?;
            let separator1 = PredefinedMenuItem::separator(app)?;
            let open_folder_item =
                MenuItem::with_id(app, "open_folder", "打开接收文件夹", true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let separator2 = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &show_item,
                    &separator1,
                    &start_item,
                    &stop_item,
                    &separator1,
                    &open_folder_item,
                    &settings_item,
                    &separator2,
                    &quit_item,
                ],
            )?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().cloned().ok_or_else(|| {
                    tauri::Error::AssetNotFound("default application icon".into())
                })?)
                .menu(&menu)
                .tooltip("LanNook — 连接附近，自由传输")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "start_service" => {
                        let _ = app.emit("tray-start-service", ());
                    }
                    "stop_service" => {
                        let _ = app.emit("tray-stop-service", ());
                    }
                    "open_folder" => {
                        let _ = app.emit("tray-open-folder", ());
                    }
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("navigate", "/settings");
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Desktop notifications + tray task counter. Subscribing to the
            // same event bus as the WebSocket fan-out keeps system
            // notifications and the tray label in sync with live transfers.
            let notification_state = state.clone();
            let notification_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                monitor_transfer_events(notification_state, notification_app).await;
            });

            // Handle close-to-tray: intercept window close request
            let window = app.get_webview_window("main").unwrap();
            let window_handle = window.clone();
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    match commands::lifecycle::read_close_behavior().as_str() {
                        "quit" => app_handle.exit(0),
                        "ask" => {
                            api.prevent_close();
                            let _ = window_handle.show();
                            let _ = window_handle.set_focus();
                            let _ = app_handle.emit("close-requested", ());
                        }
                        _ => {
                            api.prevent_close();
                            let _ = window_handle.hide();
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::server_cmd::start_local_service,
            commands::server_cmd::stop_local_service,
            commands::server_cmd::get_local_service_status,
            commands::server_cmd::refresh_local_ip,
            commands::server_cmd::regenerate_connection_token,
            commands::server_cmd::refresh_pairing_pin,
            commands::server_cmd::get_connection_info,
            commands::server_cmd::get_connection_diagnostics,
            commands::server_cmd::get_connection_qr_code,
            commands::server_cmd::configure_windows_firewall,
            commands::server_cmd::get_devices,
            commands::server_cmd::approve_device,
            commands::server_cmd::reject_device,
            commands::server_cmd::forget_device,
            commands::server_cmd::get_transfers,
            commands::server_cmd::cancel_transfer,
            commands::server_cmd::delete_transfers,
            commands::server_cmd::get_settings,
            commands::server_cmd::update_settings,
            commands::server_cmd::open_receive_folder,
            commands::transfer_cmd::send_files_to_device,
            commands::transfer_cmd::get_file_metadata,
            commands::transfer_cmd::get_pending_transfers,
            commands::transfer_cmd::pause_transfer,
            commands::transfer_cmd::resume_transfer,
            commands::lifecycle::get_autostart_enabled,
            commands::lifecycle::set_autostart,
            commands::lifecycle::get_close_behavior,
            commands::lifecycle::set_close_behavior,
            commands::lifecycle::quit_application,
            commands::lifecycle::get_app_version,
            commands::lifecycle::export_diagnostics,
            commands::lifecycle::open_log_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn app_data_base_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(|p| PathBuf::from(p).join("AppData").join("Roaming"))
                    .unwrap_or_else(|_| PathBuf::from("."))
            })
    } else {
        std::env::var("HOME")
            .map(|p| PathBuf::from(p).join(".config"))
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn move_legacy_entry(legacy: &Path, current: &Path, legacy_name: &str, current_name: &str) -> bool {
    let source = legacy.join(legacy_name);
    let target = current.join(current_name);
    if !source.exists() || target.exists() {
        return false;
    }

    if let Err(error) = std::fs::rename(&source, &target) {
        // The old data stays untouched when a migration cannot complete. The
        // application can still start with a new empty store instead of risking
        // a partial copy or destructive overwrite.
        eprintln!(
            "Could not migrate legacy LYNQO data from {} to {}: {}",
            source.display(),
            target.display(),
            error
        );
        false
    } else {
        true
    }
}

fn migrate_legacy_app_data_at(base: &Path) {
    let legacy = base.join(LEGACY_APP_DATA_DIR_NAME);
    if !legacy.exists() {
        return;
    }

    let current = base.join(APP_DATA_DIR_NAME);
    if let Err(error) = std::fs::create_dir_all(&current) {
        eprintln!(
            "Could not create LanNook data directory {}: {}",
            current.display(),
            error
        );
        return;
    }

    if move_legacy_entry(
        &legacy,
        &current,
        LEGACY_DATABASE_FILE_NAME,
        DATABASE_FILE_NAME,
    ) {
        move_legacy_entry(&legacy, &current, "lynqo.db-wal", "lannook.db-wal");
        move_legacy_entry(&legacy, &current, "lynqo.db-shm", "lannook.db-shm");
    }
    move_legacy_entry(&legacy, &current, "config.json", "config.json");
    move_legacy_entry(&legacy, &current, "logs", "logs");
}

/// Human-readable file summary for one transfer, e.g. "photo.jpg" or
/// "photo.jpg 等 3 个文件".
async fn transfer_file_label(state: &SharedState, transfer_id: &str) -> String {
    let s = state.lock().await;
    let Ok(files) = s.db.get_transfer_files(transfer_id) else {
        return transfer_id.to_string();
    };
    let Some(first) = files.first() else {
        return transfer_id.to_string();
    };
    if files.len() > 1 {
        format!("{} 等 {} 个文件", first.name, files.len())
    } else {
        first.name.clone()
    }
}

fn update_tray_label(app: &tauri::AppHandle, active_count: usize) {
    let label = if active_count > 0 {
        format!("LanNook — 正在传输 {active_count} 个任务")
    } else {
        "LanNook — 连接附近，自由传输".to_string()
    };
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(&label));
    }
}

fn send_desktop_notification(app: &tauri::AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("Failed to show desktop notification: {}", error);
    }
}

/// Subscribe to the transfer event bus and drive two desktop integrations:
///
/// * the tray tooltip shows the number of active transfers;
/// * system notifications fire on transfer start, completion and failure.
///
/// Notifications are emitted once per lifecycle transition because the event
/// bus already coalesces duplicate state changes.
async fn monitor_transfer_events(state: SharedState, app: tauri::AppHandle) {
    let mut active: HashMap<String, ()> = HashMap::new();

    // Seed the counter with transfers that are still active when the desktop
    // UI (re)starts, so the tray label survives an app relaunch.
    if let Ok(transfers) = state.lock().await.db.list_transfers() {
        for transfer in transfers {
            if matches!(
                transfer.status.as_str(),
                "transferring" | "waiting" | "accepted" | "requesting" | "awaiting_acceptance"
            ) {
                active.insert(transfer.id, ());
            }
        }
    }
    update_tray_label(&app, active.len());

    let mut rx = state.lock().await.event_tx.subscribe();
    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        };

        match &event {
            WsEvent::TransferStarted { transfer_id }
            | WsEvent::TransferResumed { transfer_id }
            | WsEvent::TransferDownloadStarted { transfer_id } => {
                active.insert(transfer_id.clone(), ());
                update_tray_label(&app, active.len());
            }
            WsEvent::TransferPaused { transfer_id }
            | WsEvent::TransferCompleted { transfer_id, .. }
            | WsEvent::TransferFailed { transfer_id, .. }
            | WsEvent::TransferCancelled { transfer_id }
            | WsEvent::TransferRejected { transfer_id }
            | WsEvent::TransferExpired { transfer_id }
            | WsEvent::TransferDeleted { transfer_id } => {
                active.remove(transfer_id);
                update_tray_label(&app, active.len());
            }
            _ => {}
        }

        match &event {
            WsEvent::TransferStarted { transfer_id }
            | WsEvent::TransferDownloadStarted { transfer_id } => {
                let label = transfer_file_label(&state, transfer_id).await;
                send_desktop_notification(&app, "传输开始", &label);
            }
            WsEvent::TransferCompleted { transfer_id, .. } => {
                let label = transfer_file_label(&state, transfer_id).await;
                send_desktop_notification(&app, "传输完成", &label);
            }
            WsEvent::TransferFailed { transfer_id, error } => {
                let label = transfer_file_label(&state, transfer_id).await;
                send_desktop_notification(&app, "传输失败", &format!("{label} — {error}"));
            }
            _ => {}
        }
    }
}

/// Returns the product data directory and migrates data from the legacy
/// LYNQO directory when this is the first LanNook launch.
pub fn get_app_data_dir() -> PathBuf {
    let base = app_data_base_dir();
    migrate_legacy_app_data_at(&base);

    let app_dir = base.join(APP_DATA_DIR_NAME);
    if let Err(error) = std::fs::create_dir_all(&app_dir) {
        eprintln!(
            "Could not create LanNook data directory {}: {}",
            app_dir.display(),
            error
        );
    }
    app_dir
}

/// Get the database file path in the app data directory.
pub fn get_db_path() -> PathBuf {
    get_app_data_dir().join(DATABASE_FILE_NAME)
}

/// Get the log directory path
pub fn get_log_dir() -> PathBuf {
    let log_dir = get_app_data_dir().join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    log_dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_base_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lannook-migration-{unique}"))
    }

    #[test]
    fn migrates_legacy_data_without_overwriting_new_files() {
        let base = temporary_base_dir();
        let legacy = base.join(LEGACY_APP_DATA_DIR_NAME);
        std::fs::create_dir_all(legacy.join("logs")).expect("create legacy directory");
        std::fs::write(legacy.join(LEGACY_DATABASE_FILE_NAME), "legacy-db")
            .expect("write legacy database");
        std::fs::write(legacy.join("config.json"), "{\"close_behavior\":\"quit\"}")
            .expect("write legacy config");
        std::fs::write(legacy.join("logs").join("lynqo.log"), "legacy-log")
            .expect("write legacy log");

        migrate_legacy_app_data_at(&base);

        let current = base.join(APP_DATA_DIR_NAME);
        assert_eq!(
            std::fs::read_to_string(current.join(DATABASE_FILE_NAME)).expect("read migrated db"),
            "legacy-db"
        );
        assert_eq!(
            std::fs::read_to_string(current.join("config.json")).expect("read migrated config"),
            "{\"close_behavior\":\"quit\"}"
        );
        assert!(current.join("logs").join("lynqo.log").exists());

        std::fs::write(current.join(DATABASE_FILE_NAME), "new-db").expect("write new db");
        std::fs::write(legacy.join(LEGACY_DATABASE_FILE_NAME), "older-db")
            .expect("write second legacy database");
        migrate_legacy_app_data_at(&base);
        assert_eq!(
            std::fs::read_to_string(current.join(DATABASE_FILE_NAME)).expect("read preserved db"),
            "new-db"
        );
        assert!(legacy.join(LEGACY_DATABASE_FILE_NAME).exists());

        std::fs::remove_dir_all(base).expect("remove temporary migration directory");
    }
}
