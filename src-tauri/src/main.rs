// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::get_rules,
            commands::get_engine_status,
            commands::store_api_key,
            commands::clear_api_key,
            commands::get_default_scan_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
