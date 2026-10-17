//! Tauri IPC commands for CodeRadar Studio.

use std::path::{Path, PathBuf};
use std::time::Duration;

use coderadar_infrastructure::{scan_project_streaming, FindingBatch, ScanReport};
use serde::Serialize;
use tauri::Emitter;

/// Engine mode status payload for the UI status bar.
#[derive(Debug, Clone, Serialize)]
pub struct MockModeStatus {
    /// True when running in Mock / Tier 0 evaluation mode (Phases 2-4).
    pub is_mock_mode: bool,
    /// Number of deterministic Tier 0 rules active.
    pub tier0_rules_count: usize,
    /// Number of heuristic mock rules active.
    pub mock_rules_count: usize,
    /// Total number of rules in active ruleset.
    pub total_rules: usize,
    /// Ruleset version token.
    pub ruleset_version: &'static str,
    /// Descriptive summary of engine status.
    pub description: &'static str,
}

/// Start a scan on `path` in `mode`, streaming batched findings to the frontend
/// every 100 ms via the IPC throttler.
#[tauri::command]
pub async fn start_scan(
    app: tauri::AppHandle,
    path: String,
    mode: String,
) -> Result<ScanReport, String> {
    if mode.eq_ignore_ascii_case("staged_diff") || mode.eq_ignore_ascii_case("staged") {
        return Err(
            "Git staged diff scan is deferred to Phase 8 (requires Git changeset engine)".into(),
        );
    }

    let root_path = PathBuf::from(&path);
    if !root_path.exists() {
        return Err(format!("Directory path does not exist: {path}"));
    }

    let app_handle = app.clone();
    let scan_result = tauri::async_runtime::spawn_blocking(move || {
        let app_for_batches = app_handle.clone();
        scan_project_streaming(
            &root_path,
            Duration::from_millis(100),
            move |batch: FindingBatch| {
                let _ = app_for_batches.emit("coderadar://scan-batch", &batch);
            },
        )
    })
    .await
    .map_err(|e| format!("Scan task join error: {e}"))?;

    match scan_result {
        Ok(report) => {
            let _ = app.emit("coderadar://scan-complete", &report);
            Ok(report)
        }
        Err(err) => {
            let error_msg = err.to_string();
            let _ = app.emit("coderadar://scan-error", &error_msg);
            Err(error_msg)
        }
    }
}

/// Retrieve the active rules manifest JSON.
#[tauri::command]
pub fn get_rules() -> Result<serde_json::Value, String> {
    const MANIFEST_JSON: &str = include_str!("../../rules/manifest.json");
    serde_json::from_str(MANIFEST_JSON).map_err(|e| e.to_string())
}

/// Retrieve current Mock Mode telemetry status.
#[tauri::command]
pub fn get_mock_mode() -> MockModeStatus {
    MockModeStatus {
        is_mock_mode: true,
        tier0_rules_count: 3,
        mock_rules_count: 13,
        total_rules: 19,
        ruleset_version: "v2026.1-strict",
        description: "Tier 0 (3 deterministic) + Mock Heuristics (13 regex) active. Jev Decision Engine integration in Phase 5.",
    }
}

/// Retrieve default workspace path or fixture path.
#[tauri::command]
pub fn get_default_scan_path() -> String {
    let fixture = Path::new("tests/fixtures/known-bad-repo");
    if fixture.exists() {
        if let Ok(abs) = std::fs::canonicalize(fixture) {
            return abs.to_string_lossy().replace('\\', "/");
        }
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| ".".into())
}
