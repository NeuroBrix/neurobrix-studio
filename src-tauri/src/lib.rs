mod app_info;
mod engine;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio_webdriver::init())
        .plugin(tauri_plugin_wdio::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![app_info::get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
