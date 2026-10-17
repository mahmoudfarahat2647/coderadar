// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::get_rules,
            commands::get_mock_mode,
            commands::get_default_scan_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
