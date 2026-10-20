//! Tauri IPC commands for CodeRadar Studio.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use coderadar_adapters::{
    unified_diff, HistoryPointDTO, PatchOutcomeDTO, PatchPreviewDTO, PatchProposalDTO,
};
use coderadar_infrastructure::{
    block_on, is_semantic_rule, mask_api_key, scan_project_streaming_with_history, CredentialStore,
    EngineStatusSnapshot, FindingBatch, JevConfig, KeyringCredentialStore, RuleManifest,
    SafePatchApplier, ScanReport, SemanticEngine, SqliteScanHistoryRepository, TIER0_OWNED_IDS,
};
use coderadar_usecases::{PatchApplier, PreparePatch, ScanHistoryRepository, SourceFileReader};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// How long the opener thread is given to install the history database.
const HISTORY_OPEN_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the waiting thread polls for the opened database.
const HISTORY_POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Handle on the scan-history database.
///
/// SQLite is a blocking driver: the database is opened on a dedicated
/// thread, never on the UI thread (`CR-SPEC-13 §6`). The repository is
/// installed in a [`OnceLock`] as soon as the opener thread is done; the
/// readers wait for it — always on a worker thread, never on the UI thread.
struct HistoryHandle {
    ready: OnceLock<SqliteScanHistoryRepository>,
}

impl HistoryHandle {
    /// A handle whose slot is still empty.
    fn new() -> Self {
        Self {
            ready: OnceLock::new(),
        }
    }

    /// Open the history database and install it in the slot.
    ///
    /// This is the body of the opener thread: it must never run on the UI
    /// thread.
    fn open(&self) {
        let repository = SqliteScanHistoryRepository::default_path()
            .and_then(SqliteScanHistoryRepository::open)
            .unwrap_or_else(|error| {
                eprintln!("[coderadar] the scan history is disabled: {error}");
                SqliteScanHistoryRepository::open_in_memory()
                    .unwrap_or_else(|fatal| panic!("cannot open the in-memory history: {fatal}"))
            });
        let _ = self.ready.set(repository);
    }

    /// The history repository, waiting for the opener thread.
    ///
    /// When the opener thread does not deliver the database in time, an
    /// in-memory database is installed as a fallback so the UI keeps working.
    fn repository(&self) -> &SqliteScanHistoryRepository {
        let deadline = Instant::now() + HISTORY_OPEN_TIMEOUT;
        while self.ready.get().is_none() && Instant::now() < deadline {
            std::thread::sleep(HISTORY_POLL_INTERVAL);
        }
        if let Some(repository) = self.ready.get() {
            return repository;
        }

        // The opener thread is stuck: fall back to an in-memory database.
        let fallback = SqliteScanHistoryRepository::open_in_memory()
            .unwrap_or_else(|fatal| panic!("cannot open the in-memory history: {fatal}"));
        let _ = self.ready.set(fallback);
        self.ready
            .get()
            .expect("the history repository is installed")
    }
}

/// Process-wide engine state shared by every command.
///
/// The [`SemanticEngine`] is swapped whenever the stored API key changes, so a
/// scan started after `store_api_key`/`clear_api_key` picks up the new engine.
pub struct AppState {
    engine: RwLock<SemanticEngine>,
    credentials: KeyringCredentialStore,
    history: Arc<HistoryHandle>,
}

impl AppState {
    /// Resolve the engine from the OS keychain / environment at startup.
    ///
    /// The history database is **not** opened here: the opener thread does it
    /// in the background so the UI thread is never blocked by SQLite.
    pub fn new() -> Self {
        let history = Arc::new(HistoryHandle::new());
        let opener = Arc::clone(&history);
        std::thread::spawn(move || opener.open());

        Self {
            engine: RwLock::new(SemanticEngine::from_environment()),
            credentials: KeyringCredentialStore::new(),
            history,
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

    let history = app.state::<AppState>().history.clone();

    let app_handle = app.clone();
    let scan_result = tauri::async_runtime::spawn_blocking(move || {
        // The repository is resolved inside the worker thread: waiting for
        // the opener thread must never happen on the UI thread.
        let repository = history.repository().clone();
        let app_for_batches = app_handle.clone();
        scan_project_streaming_with_history(
            &root_path,
            evaluator,
            Duration::from_millis(100),
            move |batch: FindingBatch| {
                let _ = app_for_batches.emit("coderadar://scan-batch", &batch);
            },
            &repository,
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

    let rebuilt = SemanticEngine::live(JevConfig {
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

/// Build the unified diff of a deterministic fix without touching the file.
///
/// The proposal is built from the current content of the file, so the preview
/// and the fingerprint it carries always describe the same bytes.
#[tauri::command]
pub async fn preview_patch(
    root_path: String,
    file_path: String,
    rule_id: String,
) -> Result<PatchPreviewDTO, String> {
    tauri::async_runtime::spawn_blocking(move || build_preview(root_path, file_path, rule_id))
        .await
        .map_err(|error| format!("Patch preview task join error: {error}"))?
}

/// Prepare the proposal and render its unified diff.
fn build_preview(
    root_path: String,
    file_path: String,
    rule_id: String,
) -> Result<PatchPreviewDTO, String> {
    let reader = Arc::new(coderadar_infrastructure::FileSystemSourceReader::new());
    let source = block_on(reader.read_source(&root_path, &file_path))
        .map_err(|error| format!("Cannot read `{file_path}`: {error}"))?;

    let prepare = PreparePatch::new(reader);
    let proposal = block_on(prepare.prepare_from_source(&source, &file_path, &rule_id))
        .map_err(|error| format!("No deterministic fix is available: {error}"))?;

    let diff = unified_diff(&source.content, &proposal.new_content, &file_path);
    let title = coderadar_usecases::template_for(&rule_id)
        .map(|template| template.title.to_string())
        .unwrap_or_else(|| format!("Deterministic fix for {rule_id}"));

    Ok(PatchPreviewDTO {
        file_path: proposal.file_path.clone(),
        rule_id: proposal.rule_id.clone(),
        title,
        explanation: proposal.explanation.clone(),
        original_sha256: proposal.original_sha256.clone(),
        new_sha256: coderadar_infrastructure::sha256_hex(&proposal.new_content),
        original_line_count: proposal.original_line_count,
        unified_diff: diff.text.clone(),
        added_lines: diff.added_lines,
        removed_lines: diff.removed_lines,
        warnings: Vec::new(),
        proposal: PatchProposalDTO::from_proposal(&proposal, diff.text),
    })
}

/// Apply a reviewed patch to the workspace.
///
/// The applier refuses the patch when the file changed since the preview (the
/// SHA-256 fingerprint does not match) and always writes atomically.
#[tauri::command]
pub async fn apply_patch(
    root_path: String,
    proposal: PatchProposalDTO,
) -> Result<PatchOutcomeDTO, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let applier = SafePatchApplier::new();
        let domain_proposal = proposal.to_proposal();
        let outcome = block_on(applier.apply_patch(&root_path, &domain_proposal))
            .map_err(|error| format!("The patch could not be applied: {error}"))?;
        Ok(PatchOutcomeDTO::from_outcome(&outcome))
    })
    .await
    .map_err(|error| format!("Patch task join error: {error}"))?
}

/// Load the compliance trend of the past scans, newest first.
///
/// When `root_path` is given, only the sessions of that workspace are
/// returned, so the trend never mixes projects.
#[tauri::command]
pub async fn get_scan_history(
    state: tauri::State<'_, AppState>,
    limit: usize,
    root_path: Option<String>,
) -> Result<Vec<HistoryPointDTO>, String> {
    let history = state.history.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // The repository is resolved inside the worker thread: waiting for
        // the opener thread must never happen on the UI thread.
        let repository = history.repository().clone();
        let root = root_path
            .as_deref()
            .map(str::trim)
            .filter(|root| !root.is_empty())
            .map(str::to_string);
        let summaries = block_on(repository.get_history(limit, root.as_deref()))
            .map_err(|error| format!("The scan history cannot be read: {error}"))?;
        Ok(summaries
            .iter()
            .map(HistoryPointDTO::from_summary)
            .collect::<Vec<_>>())
    })
    .await
    .map_err(|error| format!("History task join error: {error}"))?
}
