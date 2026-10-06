mod app_info;
mod architecture;
mod engine;

use engine::EngineAvailability;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio_webdriver::init())
        .plugin(tauri_plugin_wdio::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_info::get_app_info,
            engine_discovery
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
