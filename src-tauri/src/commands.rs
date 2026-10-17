//! Tauri IPC commands for CodeRadar Studio.

use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

use coderadar_infrastructure::{
    is_semantic_rule, mask_api_key, scan_project_streaming_with, CredentialStore,
    EngineStatusSnapshot, FindingBatch, JevConfig, KeyringCredentialStore, RuleManifest,
    ScanReport, SemanticEngine, TIER0_OWNED_IDS,
};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// Process-wide engine state shared by every command.
///
/// The [`SemanticEngine`] is swapped whenever the stored API key changes, so a
/// scan started after `store_api_key`/`clear_api_key` picks up the new engine.
pub struct AppState {
    engine: RwLock<SemanticEngine>,
    credentials: KeyringCredentialStore,
}

impl AppState {
    /// Resolve the engine from the OS keychain / environment at startup.
    pub fn new() -> Self {
        Self {
            engine: RwLock::new(SemanticEngine::from_environment()),
            credentials: KeyringCredentialStore::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Engine telemetry payload for the UI status bar: the engine snapshot flattened
/// together with keychain and ruleset details.
#[derive(Debug, Clone, Serialize)]
pub struct EngineStatusPayload {
    #[serde(flatten)]
    engine: EngineStatusSnapshot,
    /// Whether an API key is configured.
    api_key_configured: bool,
    /// Masked API key for display, e.g. `sk-or-…`.
    api_key_masked: Option<String>,
    /// Number of deterministic Tier 0 rules active.
    tier0_rules_count: usize,
    /// Number of semantic (Mock/Jev) rules active.
    semantic_rules_count: usize,
    /// Total number of rules in the active ruleset.
    total_rules: usize,
    /// Ruleset version token.
    ruleset_version: String,
    /// Human-readable summary of engine status.
    description: String,
}

impl EngineStatusPayload {
    /// Build the payload from an engine snapshot plus keychain/ruleset details.
    fn from_snapshot(snapshot: EngineStatusSnapshot, key: Option<String>) -> Self {
        let manifest = RuleManifest::embedded().ok();
        let rules = manifest
            .as_ref()
            .map(|manifest| manifest.rules.as_slice())
            .unwrap_or(&[]);
        let tier0_rules_count = rules
            .iter()
            .filter(|rule| TIER0_OWNED_IDS.contains(&rule.id.as_str()))
            .count();
        let semantic_rules_count = rules.iter().filter(|rule| is_semantic_rule(rule)).count();
        let total_rules = rules.len();
        let ruleset_version = manifest
            .as_ref()
            .map(|manifest| manifest.ruleset_version.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let description = format!(
            "{}: {} Tier 0 (deterministic) + {} semantic rules active.",
            snapshot.badge, tier0_rules_count, semantic_rules_count
        );

        Self {
            api_key_configured: key.is_some(),
            api_key_masked: key.as_deref().map(mask_api_key),
            tier0_rules_count,
            semantic_rules_count,
            total_rules,
            ruleset_version,
            description,
            engine: snapshot,
        }
    }
}

/// Read the current engine + keychain status.
fn current_status(state: &AppState) -> EngineStatusPayload {
    let snapshot = state
        .engine
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .status()
        .snapshot();
    let key = state.credentials.get_api_key().ok().flatten();
    EngineStatusPayload::from_snapshot(snapshot, key)
}

/// Scan mode requested by the UI; `staged_diff` is Phase 8 scope.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanModeArg {
    /// Full project walk.
    FullScan,
    /// Git staged changes only (not implemented yet).
    StagedDiff,
}

/// Start a scan on `path` in `mode`, streaming batched findings to the frontend
/// every 100 ms via the IPC throttler.
#[tauri::command]
pub async fn start_scan(
    app: tauri::AppHandle,
    path: String,
    mode: ScanModeArg,
) -> Result<ScanReport, String> {
    if let ScanModeArg::StagedDiff = mode {
        return Err(
            "Git staged diff scan is deferred to Phase 8 (requires Git changeset engine)".into(),
        );
    }

    let root_path = PathBuf::from(&path);
    if !root_path.exists() {
        return Err(format!("Directory path does not exist: {path}"));
    }

    let evaluator = app
        .state::<AppState>()
        .engine
        .read()
        .map_err(|_| "Engine state lock poisoned".to_string())?
        .evaluator();

    let app_handle = app.clone();
    let scan_result = tauri::async_runtime::spawn_blocking(move || {
        let app_for_batches = app_handle.clone();
        scan_project_streaming_with(
            &root_path,
            evaluator,
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

/// Retrieve the current engine telemetry (mode, badge, latency, key state).
#[tauri::command]
pub fn get_engine_status(state: tauri::State<'_, AppState>) -> EngineStatusPayload {
    current_status(&state)
}

/// Persist the Jev / OpenRouter API key in the OS keychain and rebuild the
/// engine so the next scan runs live.
#[tauri::command]
pub fn store_api_key(
    state: tauri::State<'_, AppState>,
    api_key: String,
) -> Result<EngineStatusPayload, String> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err("API key must not be empty".to_string());
    }
    state
        .credentials
        .set_api_key(trimmed)
        .map_err(|error| error.to_string())?;

    let rebuilt =
        SemanticEngine::live(JevConfig {
            api_key: trimmed.to_string(),
            ..JevConfig::default()
        })
        .unwrap_or_else(|_| SemanticEngine::mock());

    let mut engine = state
        .engine
        .write()
        .map_err(|_| "Engine state lock poisoned".to_string())?;
    *engine = rebuilt;
    drop(engine);

    Ok(current_status(&state))
}

/// Remove the stored API key and revert to the offline Mock engine.
#[tauri::command]
pub fn clear_api_key(state: tauri::State<'_, AppState>) -> Result<EngineStatusPayload, String> {
    state
        .credentials
        .delete_api_key()
        .map_err(|error| error.to_string())?;

    let mut engine = state
        .engine
        .write()
        .map_err(|_| "Engine state lock poisoned".to_string())?;
    *engine = SemanticEngine::from_environment();
    drop(engine);

    Ok(current_status(&state))
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
