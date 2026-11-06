// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    let probe_start = std::time::Instant::now();
    let bootmark = matches!(std::env::var("CODERADAR_BOOTMARK"), Ok(value) if !value.is_empty());
    let idle_ms = std::env::var("CODERADAR_IDLE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::get_rules,
            commands::get_engine_status,
            commands::check_provider_diagnostic,
            commands::store_api_key,
            commands::clear_api_key,
            commands::get_default_scan_path,
            commands::preview_patch,
            commands::apply_patch,
            commands::get_scan_history,
            commands::export_report,
            commands::get_tier2_status,
            commands::store_tier2_key,
            commands::clear_tier2_key,
            commands::generate_fix
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(move |app_handle, event| {
        if let tauri::RunEvent::Ready = event {
            if bootmark {
                eprintln!("bootmark={}", probe_start.elapsed().as_millis());
            }
            if let Some(idle_ms) = idle_ms {
                let app_handle = app_handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(idle_ms));
                    app_handle.exit(0);
                });
            }
        }
    });
}
