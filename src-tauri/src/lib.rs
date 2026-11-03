mod app_info;
mod architecture;
mod engine;

use engine::daemon::{DaemonManager, DaemonStatus, MutationFailure};
use engine::EngineAvailability;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
async fn engine_discovery() -> EngineAvailability {
    tauri::async_runtime::spawn_blocking(engine::discover)
        .await
        .unwrap_or_else(|_| EngineAvailability::Unavailable {
            reason: "The engine status check did not complete.".into(),
            diagnostics: None,
        })
}

/// Read-only: what answers on Studio's engine instance. A read never
/// starts or stops anything — mutations happen only on explicit user
/// action. The `Result` wrapper satisfies Tauri's requirement for async
/// commands with borrowed state; the error arm is never constructed.
#[tauri::command]
async fn daemon_status(
    manager: tauri::State<'_, DaemonManager>,
) -> Result<DaemonStatus, MutationFailure> {
    Ok(manager.read_status().await)
}

/// Start an owned daemon for an explicit model id, through the public CLI
/// only. Rejections are structured failures, never swallowed.
#[tauri::command]
async fn start_owned_daemon(
    manager: tauri::State<'_, DaemonManager>,
    model_id: String,
) -> Result<DaemonStatus, MutationFailure> {
    manager.start(model_id).await
}

/// Stop the daemon this session owns — never one it does not. A stop is
/// confirmed by process exit and instance-file cleanup; a timeout is
/// reported as not stopped.
#[tauri::command]
async fn stop_owned_daemon(
    manager: tauri::State<'_, DaemonManager>,
) -> Result<DaemonStatus, MutationFailure> {
    manager.stop().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio_webdriver::init())
        .plugin(tauri_plugin_wdio::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // The Studio-named instance lives under the application cache
            // directory — Tauri's path API, never a hand-built location.
            let cache_dir = app.path().app_cache_dir()?;
            app.manage(DaemonManager::new(&cache_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info::get_app_info,
            engine_discovery,
            daemon_status,
            start_owned_daemon,
            stop_owned_daemon
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
