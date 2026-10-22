//! Tauri IPC commands for CodeRadar Studio.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use coderadar_adapters::{
    unified_diff, HistoryPointDTO, PatchOutcomeDTO, PatchPreviewDTO, PatchProposalDTO,
};
use coderadar_domain::ScanMode;
use coderadar_infrastructure::{
    block_on, is_semantic_rule, mask_api_key, resolve_gemini_key,
    scan_project_streaming_with_history_mode, write_markdown_report, CredentialStore,
    EngineStatusSnapshot, FindingBatch, GeminiFixProvider, JevConfig,
    KeyringCredentialStore, OllamaFixProvider, RulesStore, SafePatchApplier,
    ScanReport, SemanticEngine, SqliteScanHistoryRepository, DEFAULT_GEMINI_MODEL,
    DEFAULT_OLLAMA_ENDPOINT, DEFAULT_OLLAMA_MODEL, GEMINI_MODEL_ENV, OLLAMA_ENDPOINT_ENV,
    OLLAMA_MODEL_ENV, TIER0_OWNED_IDS,
};
use coderadar_usecases::{
    FixFinding, GenerateFix, GenerativeFixProvider, PatchApplier, PreparePatch,
    ScanHistoryRepository, SourceFileReader,
};
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
    tier2_credentials: KeyringCredentialStore,
    history: Arc<HistoryHandle>,
    /// The report of the last finished scan, kept so the UI can export it as a
    /// Markdown document. It is `None` until the first scan succeeds.
    last_report: RwLock<Option<ScanReport>>,
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
            tier2_credentials: KeyringCredentialStore::gemini(),
            history,
            last_report: RwLock::new(None),
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
        let active = RulesStore::new(RulesStore::default_data_dir()).active_or_embedded();
        let manifest = active.manifest;
        let rules = manifest.rules.as_slice();
        let tier0_rules_count = rules
            .iter()
            .filter(|rule| TIER0_OWNED_IDS.contains(&rule.id.as_str()))
            .count();
        let semantic_rules_count = rules.iter().filter(|rule| is_semantic_rule(rule)).count();
        let total_rules = rules.len();
        let ruleset_version = manifest.ruleset_version.clone();

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

/// Scan mode requested by the UI.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanModeArg {
    /// Full project walk.
    FullScan,
    /// Git staged changes only, with the Pull Request hygiene scorecard.
    StagedDiff,
}

impl From<ScanModeArg> for ScanMode {
    fn from(mode: ScanModeArg) -> Self {
        match mode {
            ScanModeArg::FullScan => Self::FullScan,
            ScanModeArg::StagedDiff => Self::StagedDiff,
        }
    }
}

/// Start a scan on `path` in `mode`, streaming batched findings to the frontend
/// every 100 ms via the IPC throttler.
#[tauri::command]
pub async fn start_scan(
    app: tauri::AppHandle,
    path: String,
    mode: ScanModeArg,
) -> Result<ScanReport, String> {
    let scan_mode = ScanMode::from(mode);

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
        scan_project_streaming_with_history_mode(
            &root_path,
            evaluator,
            Duration::from_millis(100),
            scan_mode,
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
            // Keep the finished report so the UI can export it as Markdown.
            let state = app.state::<AppState>();
            let mut last_report = state
                .last_report
                .write()
                .map_err(|_| "Engine state lock poisoned".to_string())?;
            *last_report = Some(report.clone());
            drop(last_report);

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

/// Retrieve the active rules manifest JSON (installed OTA ruleset or embedded).
#[tauri::command]
pub fn get_rules() -> Result<serde_json::Value, String> {
    let json = RulesStore::new(RulesStore::default_data_dir()).active_json();
    serde_json::from_str(&json).map_err(|e| e.to_string())
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

/// Tier 2 provider configuration shown in the settings panel.
#[derive(Debug, Clone, Serialize)]
pub struct Tier2StatusPayload {
    /// Token of the local provider (`ollama`).
    pub local_provider: &'static str,
    /// Local model name.
    pub local_model: String,
    /// Local Ollama endpoint.
    pub local_endpoint: String,
    /// Token of the cloud provider (`gemini`).
    pub cloud_provider: &'static str,
    /// Cloud model name.
    pub cloud_model: String,
    /// Whether a Gemini key is stored in the keychain or the environment.
    pub cloud_key_configured: bool,
}

fn tier2_status() -> Tier2StatusPayload {
    let local_model =
        std::env::var(OLLAMA_MODEL_ENV).unwrap_or_else(|_| DEFAULT_OLLAMA_MODEL.to_string());
    let local_endpoint =
        std::env::var(OLLAMA_ENDPOINT_ENV).unwrap_or_else(|_| DEFAULT_OLLAMA_ENDPOINT.to_string());
    let cloud_model =
        std::env::var(GEMINI_MODEL_ENV).unwrap_or_else(|_| DEFAULT_GEMINI_MODEL.to_string());
    let cloud_key_configured =
        resolve_gemini_key(&KeyringCredentialStore::gemini()).is_some();

    Tier2StatusPayload {
        local_provider: "ollama",
        local_model,
        local_endpoint,
        cloud_provider: "gemini",
        cloud_model,
        cloud_key_configured,
    }
}

/// Read the Tier 2 provider configuration.
#[tauri::command]
pub fn get_tier2_status() -> Tier2StatusPayload {
    tier2_status()
}

/// Store the Gemini API key in the OS keychain.
#[tauri::command]
pub fn store_tier2_key(
    state: tauri::State<'_, AppState>,
    api_key: String,
) -> Result<Tier2StatusPayload, String> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err("API key must not be empty".to_string());
    }
    state
        .tier2_credentials
        .set_api_key(trimmed)
        .map_err(|error| error.to_string())?;
    Ok(tier2_status())
}

/// Remove the stored Gemini API key.
#[tauri::command]
pub fn clear_tier2_key(state: tauri::State<'_, AppState>) -> Result<Tier2StatusPayload, String> {
    state
        .tier2_credentials
        .delete_api_key()
        .map_err(|error| error.to_string())?;
    Ok(tier2_status())
}

/// A Tier 2 generation request from the UI.
#[derive(Debug, Clone, Deserialize)]
pub struct Tier2FixRequestDTO {
    /// Workspace root.
    pub root_path: String,
    /// Workspace-relative file path.
    pub file_path: String,
    /// Rule identifier.
    pub rule_id: String,
    /// Rule title.
    pub title: String,
    /// Finding description.
    pub description: String,
    /// First offending line.
    pub line_start: u32,
    /// Last offending line.
    pub line_end: u32,
    /// `ollama` or `gemini`.
    pub provider: String,
    /// Explicit consent to send the file to a cloud model.
    #[serde(default)]
    pub allow_cloud: bool,
}

/// Generate a Tier 2 fix and return it as a reviewable diff preview.
///
/// Nothing is written: the proposal is rendered exactly like a deterministic
/// fix, so the user reviews the diff and applies it through `apply_patch`.
#[tauri::command]
pub async fn generate_fix(request: Tier2FixRequestDTO) -> Result<PatchPreviewDTO, String> {
    tauri::async_runtime::spawn_blocking(move || build_tier2_preview(request))
        .await
        .map_err(|error| format!("Tier 2 task join error: {error}"))?
}

fn build_tier2_preview(request: Tier2FixRequestDTO) -> Result<PatchPreviewDTO, String> {
    let provider: Arc<dyn GenerativeFixProvider> = match request.provider.as_str() {
        "ollama" => Arc::new(
            OllamaFixProvider::from_env().map_err(|error| error.to_string())?,
        ),
        "gemini" => {
            if !request.allow_cloud {
                return Err(
                    "Cloud fix generation needs explicit consent: enable `allow_cloud` after \
                     reviewing the privacy notice."
                        .to_string(),
                );
            }
            let key = resolve_gemini_key(&KeyringCredentialStore::gemini()).ok_or_else(|| {
                "No Gemini API key is configured. Add one in Settings or use the local \
                 Ollama provider."
                    .to_string()
            })?;
            Arc::new(GeminiFixProvider::from_key(key).map_err(|error| error.to_string())?)
        }
        other => {
            return Err(format!(
                "Unknown Tier 2 provider `{other}` (expected `ollama` or `gemini`)"
            ))
        }
    };

    let reader: Arc<dyn SourceFileReader> = Arc::new(coderadar_infrastructure::FileSystemSourceReader::new());
    let generator = GenerateFix::new(provider, Arc::clone(&reader));
    let finding = FixFinding {
        file_path: request.file_path.clone(),
        rule_id: request.rule_id.clone(),
        title: request.title.clone(),
        description: request.description.clone(),
        line_start: request.line_start,
        line_end: request.line_end,
    };
    let generated = block_on(generator.generate(&request.root_path, &finding))
        .map_err(|error| format!("The Tier 2 model cannot produce a fix: {error}"))?;
    let source = block_on(reader.read_source(&request.root_path, &request.file_path))
        .map_err(|error| format!("Cannot read `{}`: {error}", request.file_path))?;

    Ok(preview_from_proposal(
        generated.proposal,
        &source.content,
        format!("Tier 2 fix for {}", request.rule_id),
        generated.warnings,
    ))
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
///
/// A finding without a deterministic fix is not an error: the preview carries
/// no proposal and the modal explains why (`CR-SPEC-13 §5`).
fn build_preview(
    root_path: String,
    file_path: String,
    rule_id: String,
) -> Result<PatchPreviewDTO, String> {
    let reader = Arc::new(coderadar_infrastructure::FileSystemSourceReader::new());
    let source = block_on(reader.read_source(&root_path, &file_path))
        .map_err(|error| format!("Cannot read `{file_path}`: {error}"))?;

    if coderadar_usecases::template_for(&rule_id).is_none() {
        return Ok(no_proposal_preview(
            &source,
            &file_path,
            &rule_id,
            format!("No deterministic fix template exists for rule `{rule_id}`."),
            Vec::new(),
        ));
    }

    let prepare = PreparePatch::new(reader);
    let proposal = match block_on(prepare.prepare_from_source(&source, &file_path, &rule_id)) {
        Ok(proposal) => proposal,
        Err(error) => {
            return Ok(no_proposal_preview(
                &source,
                &file_path,
                &rule_id,
                format!(
                "The deterministic template for rule `{rule_id}` does not match this file exactly."
            ),
                vec![error.to_string()],
            ))
        }
    };

    let title = coderadar_usecases::template_for(&rule_id)
        .map(|template| template.title.to_string())
        .unwrap_or_else(|| format!("Deterministic fix for {rule_id}"));

    Ok(preview_from_proposal(
        proposal,
        &source.content,
        title,
        Vec::new(),
    ))
}

/// Render a proposal against the original content as a UI preview.
fn preview_from_proposal(
    proposal: coderadar_domain::PatchProposal,
    original_content: &str,
    title: String,
    warnings: Vec<String>,
) -> PatchPreviewDTO {
    let diff = unified_diff(original_content, &proposal.new_content, &proposal.file_path);
    PatchPreviewDTO {
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
        warnings,
        proposal: Some(PatchProposalDTO::from_proposal(&proposal, diff.text)),
    }
}

/// Build the preview of a finding that has no deterministic fix.
///
/// The preview is still rendered - with the fingerprint of the untouched file
/// and without a proposal - so the modal can explain that no deterministic
/// fix exists for this finding (`CR-SPEC-13 §5`).
fn no_proposal_preview(
    source: &coderadar_usecases::SourceFile,
    file_path: &str,
    rule_id: &str,
    explanation: String,
    warnings: Vec<String>,
) -> PatchPreviewDTO {
    PatchPreviewDTO {
        file_path: file_path.to_string(),
        rule_id: rule_id.to_string(),
        title: "No deterministic fix available".to_string(),
        explanation,
        original_sha256: source.sha256.clone(),
        new_sha256: source.sha256.clone(),
        original_line_count: source.line_count,
        unified_diff: String::new(),
        added_lines: 0,
        removed_lines: 0,
        warnings,
        proposal: None,
    }
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

/// The outcome of a Markdown report export (`FR-07`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOutcomeDTO {
    /// Absolute path of the written report.
    pub path: String,
    /// Size of the report in bytes.
    pub bytes: usize,
    /// Number of findings the report describes.
    pub findings: usize,
}

/// Export the last scan as a Markdown report at `target_path`.
///
/// The report is the one of the last finished scan; the write is atomic, so
/// the target file is never observed half written (`CR-SPEC-13 §4`).
#[tauri::command]
pub async fn export_report(
    state: tauri::State<'_, AppState>,
    target_path: String,
) -> Result<ExportOutcomeDTO, String> {
    // The report is copied out of the state before the blocking write: the
    // lock must never be held across an await point.
    let report = {
        let last_report = state
            .last_report
            .read()
            .map_err(|_| "Engine state lock poisoned".to_string())?;
        last_report.clone().ok_or_else(|| {
            "No scan has been run yet: run a scan before exporting a report.".to_string()
        })?
    };

    tauri::async_runtime::spawn_blocking(move || {
        let outcome = write_markdown_report(Path::new(&target_path), &report)
            .map_err(|error| format!("The report cannot be written: {error}"))?;
        Ok(ExportOutcomeDTO {
            path: outcome.path,
            bytes: outcome.bytes,
            findings: outcome.findings,
        })
    })
    .await
    .map_err(|error| format!("Report export task join error: {error}"))?
}
