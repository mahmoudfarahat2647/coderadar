//! Tauri IPC commands for CodeRadar Studio.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use coderadar_adapters::{
    unified_diff, HistoryPointDTO, PatchOutcomeDTO, PatchPreviewDTO, PatchProposalDTO,
};
use coderadar_domain::ScanMode;
use coderadar_infrastructure::{
    block_on, is_semantic_rule, resolve_gemini_key,
    scan_project_provisional_with_engine_history_status, write_markdown_report, CredentialStore,
    EngineStatusSnapshot, FindingBatch, GeminiFixProvider, JevConfig, JevDiagnostics,
    KeyringCredentialStore, OllamaFixProvider, ProviderDiagnostic, RulesStore, SafePatchApplier,
    ScanReport, SemanticEngine, SqliteScanHistoryRepository, DEFAULT_GEMINI_MODEL,
    DEFAULT_OLLAMA_ENDPOINT, DEFAULT_OLLAMA_MODEL, GEMINI_MODEL_ENV, OLLAMA_ENDPOINT_ENV,
    OLLAMA_MODEL_ENV, TIER0_OWNED_IDS,
};
use coderadar_usecases::{
    BoxFuture, FixFinding, GenerateFix, GenerativeFixProvider, PatchApplier, PreparePatch,
    ScanHistoryRepository, SourceFile, SourceFileReader,
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
    ready: OnceLock<(SqliteScanHistoryRepository, bool)>,
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
            .map(|repository| (repository, true))
            .unwrap_or_else(|error| {
                eprintln!("[coderadar] the scan history is disabled: {error}");
                (
                    SqliteScanHistoryRepository::open_in_memory().unwrap_or_else(|fatal| {
                        panic!("cannot open the in-memory history: {fatal}")
                    }),
                    false,
                )
            });
        let _ = self.ready.set(repository);
    }

    /// The history repository, waiting for the opener thread.
    ///
    /// When the opener thread does not deliver the database in time, an
    /// in-memory database is installed as a fallback so the UI keeps working.
    fn repository(&self) -> &(SqliteScanHistoryRepository, bool) {
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
        let _ = self.ready.set((fallback, false));
        self.ready
            .get()
            .expect("the history repository is installed")
    }
}

/// Desktop scan response with history durability separate from scan success.
#[derive(Clone, Serialize)]
pub struct DesktopScanReport {
    #[serde(flatten)]
    report: ScanReport,
    history_status: &'static str,
}

/// Process-wide engine state shared by every command.
///
/// The [`SemanticEngine`] is swapped whenever the stored API key changes, so a
/// scan started after `store_api_key`/`clear_api_key` picks up the new engine.
pub struct AppState {
    engine: RwLock<Arc<SemanticEngine>>,
    credentials: KeyringCredentialStore,
    tier2_credentials: KeyringCredentialStore,
    history: Arc<HistoryHandle>,
    /// The report of the last finished scan, kept so the UI can export it as a
    /// Markdown document. It is `None` until the first scan succeeds.
    last_report: RwLock<Option<ScanReport>>,
    fix_authorization: Arc<RwLock<FixAuthorization>>,
}

#[derive(Clone)]
struct AuthorizedFinding {
    id: String,
    file: String,
    rule_id: String,
    title: String,
    description: String,
    line_start: u32,
    line_end: u32,
    sha256: String,
}

#[derive(Default)]
struct FixAuthorization {
    generation: u64,
    root: Option<String>,
    findings: Vec<AuthorizedFinding>,
    issued_proposals: Vec<PatchProposalDTO>,
}

impl FixAuthorization {
    fn begin_scan(&mut self) -> u64 {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("scan generation exhausted");
        self.root = None;
        self.findings.clear();
        self.issued_proposals.clear();
        self.generation
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    fn finish_scan(
        &mut self,
        generation: u64,
        root: String,
        full: bool,
        findings: Vec<AuthorizedFinding>,
    ) {
        if self.generation == generation && full {
            self.root = Some(root);
            self.findings = findings;
        }
    }

    fn find(
        &self,
        root: &str,
        id: &str,
        file: &str,
        rule: &str,
    ) -> Result<&AuthorizedFinding, String> {
        self.findings.iter().find(|finding| {
            self.root.as_deref() == Some(root)
                && finding.id == id && finding.file == file && finding.rule_id == rule
        }).ok_or_else(|| "Fixes are available only for findings from the latest completed full scan of this workspace; staged or unrecognized findings are refused.".to_string())
    }

    fn issue(
        &mut self,
        generation: u64,
        root: &str,
        finding: &AuthorizedFinding,
        proposal: &PatchProposalDTO,
    ) -> Result<(), String> {
        if self.generation != generation
            || self
                .find(root, &finding.id, &finding.file, &finding.rule_id)?
                .sha256
                != finding.sha256
            || proposal.file_path != finding.file
            || proposal.rule_id != finding.rule_id
            || proposal.original_sha256 != finding.sha256
        {
            return Err(
                "Fix authorization expired or proposal does not match the scanned source".into(),
            );
        }
        if !self.issued_proposals.contains(proposal) {
            if self.issued_proposals.len() >= 32 {
                return Err("Too many outstanding patch previews; start a new scan".into());
            }
            self.issued_proposals.push(proposal.clone());
        }
        Ok(())
    }

    fn issued(&self, root: &str, proposal: &PatchProposalDTO) -> Result<&PatchProposalDTO, String> {
        self.issued_proposals
            .iter()
            .find(|issued| self.root.as_deref() == Some(root) && *issued == proposal)
            .ok_or_else(|| "Patch was not issued for the latest completed scan".to_string())
    }

    fn take_issued(
        &mut self,
        root: &str,
        proposal: &PatchProposalDTO,
    ) -> Result<PatchProposalDTO, String> {
        self.issued(root, proposal)?;
        let index = self
            .issued_proposals
            .iter()
            .position(|issued| issued == proposal)
            .expect("issued proposal exists");
        Ok(self.issued_proposals.remove(index))
    }
}

fn verify_snapshot(expected: &str, content: &str) -> Result<(), String> {
    if coderadar_infrastructure::sha256_hex(content) != expected {
        return Err(
            "Source changed since the scan; run a new full scan before proposing a fix".into(),
        );
    }
    Ok(())
}

struct SnapshotReader(SourceFile);

impl SourceFileReader for SnapshotReader {
    fn read_source<'a>(
        &'a self,
        _root: &'a str,
        _relative_path: &'a str,
    ) -> BoxFuture<'a, Result<SourceFile, coderadar_domain::DomainError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

fn read_verified_source(root: &str, finding: &AuthorizedFinding) -> Result<SourceFile, String> {
    let reader = coderadar_infrastructure::FileSystemSourceReader::new();
    let source = block_on(reader.read_source(root, &finding.file))
        .map_err(|error| format!("Cannot read `{}`: {error}", finding.file))?;
    verify_snapshot(&finding.sha256, &source.content)?;
    Ok(source)
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
            engine: RwLock::new(Arc::new(SemanticEngine::from_environment())),
            credentials: KeyringCredentialStore::new(),
            tier2_credentials: KeyringCredentialStore::gemini(),
            history,
            last_report: RwLock::new(None),
            fix_authorization: Arc::new(RwLock::new(FixAuthorization::default())),
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
    fn from_snapshot(mut snapshot: EngineStatusSnapshot, key_configured: bool) -> Self {
        // Provider errors must never cross the webview boundary.
        snapshot.reason = snapshot.reason.map(|_| "Jev request failed".to_string());
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
            api_key_configured: key_configured,
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
    let key_configured = state.credentials.get_api_key().ok().flatten().is_some();
    EngineStatusPayload::from_snapshot(snapshot, key_configured)
}

/// Check provider authentication or explicitly run a fixed synthetic inference.
#[tauri::command]
pub async fn check_provider_diagnostic(synthetic: bool) -> Result<ProviderDiagnostic, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let diagnostics = JevDiagnostics::from_environment();
        if synthetic {
            diagnostics.check_synthetic()
        } else {
            diagnostics.check_metadata()
        }
    })
    .await
    .map_err(|_| "Provider check could not be completed".to_string())
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
    event_token: Option<String>,
) -> Result<DesktopScanReport, String> {
    let authorization = app.state::<AppState>().fix_authorization.clone();
    let generation = authorization
        .write()
        .map_err(|_| "Fix authorization lock poisoned".to_string())?
        .begin_scan();
    let scan_mode = ScanMode::from(mode);
    if event_token.as_ref().is_some_and(|token| {
        token.is_empty()
            || token.len() > 64
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return Err("Invalid scan event token".into());
    }
    let event_name = |kind: &str| match &event_token {
        Some(token) => format!("coderadar://scan-{kind}-{token}"),
        None => format!("coderadar://scan-{kind}"),
    };
    let batch_event = event_name("provisional");
    let complete_event = event_name("complete");
    let error_event = event_name("error");

    let root_path = PathBuf::from(&path);
    if !root_path.exists() {
        return Err(format!("Directory path does not exist: {path}"));
    }

    let engine = {
        let state = app.state::<AppState>();
        let engine = state
            .engine
            .read()
            .map_err(|_| "Engine state lock poisoned".to_string())?;
        Arc::clone(&engine)
    };

    let history = app.state::<AppState>().history.clone();

    let app_handle = app.clone();
    let batch_authorization = authorization.clone();
    let scan_result = tauri::async_runtime::spawn_blocking(move || {
        // The repository is resolved inside the worker thread: waiting for
        // the opener thread must never happen on the UI thread.
        let (repository, durable) = history.repository();
        let app_for_batches = app_handle;
        scan_project_provisional_with_engine_history_status(
            &root_path,
            &engine,
            Duration::from_millis(100),
            scan_mode,
            move |batch: FindingBatch| {
                if let Ok(auth) = batch_authorization.read() {
                    if auth.is_current(generation) {
                        let _ = app_for_batches.emit(&batch_event, &batch);
                    }
                }
            },
            repository,
        )
        .map(|(report, saved)| (report, *durable, saved))
    })
    .await
    .map_err(|e| format!("Scan task join error: {e}"))?;

    match scan_result {
        Ok((report, durable, saved)) => {
            let mut auth = authorization
                .write()
                .map_err(|_| "Fix authorization lock poisoned".to_string())?;
            if auth.generation != generation {
                return Err("Scan superseded by a newer scan".to_string());
            }
            let findings = report
                .findings
                .iter()
                .filter_map(|finding| {
                    Some(AuthorizedFinding {
                        id: finding.id.clone(),
                        file: finding.file.clone(),
                        rule_id: finding.rule_id.clone(),
                        title: finding.title.clone(),
                        description: finding.description.clone(),
                        line_start: finding.line_start,
                        line_end: finding.line_end,
                        sha256: report
                            .scan
                            .analyzed_sources
                            .as_ref()?
                            .get(&finding.file)?
                            .clone(),
                    })
                })
                .collect();
            auth.finish_scan(
                generation,
                path,
                report.session.mode == "full_scan",
                findings,
            );
            // Keep the finished report so the UI can export it as Markdown.
            let state = app.state::<AppState>();
            let mut last_report = state
                .last_report
                .write()
                .map_err(|_| "Engine state lock poisoned".to_string())?;
            *last_report = Some(report.clone());
            drop(last_report);

            let response = DesktopScanReport {
                report,
                history_status: if !saved {
                    "save_failed"
                } else if !durable {
                    "in_memory"
                } else {
                    "saved"
                },
            };
            let _ = app.emit(&complete_event, &response);
            drop(auth);
            Ok(response)
        }
        Err(err) => {
            let error_msg = err.to_string();
            if let Ok(auth) = authorization.read() {
                if auth.is_current(generation) {
                    let _ = app.emit(&error_event, &error_msg);
                }
            }
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
        .map_err(|_| "Could not store the API key in the keychain".to_string())?;

    let rebuilt = SemanticEngine::live(JevConfig {
        api_key: trimmed.to_string(),
        ..JevConfig::default()
    })
    .unwrap_or_else(|_| SemanticEngine::mock());

    let mut engine = state
        .engine
        .write()
        .map_err(|_| "Engine state lock poisoned".to_string())?;
    *engine = Arc::new(rebuilt);
    drop(engine);

    Ok(current_status(&state))
}

/// Remove the stored API key and revert to the offline Mock engine.
#[tauri::command]
pub fn clear_api_key(state: tauri::State<'_, AppState>) -> Result<EngineStatusPayload, String> {
    state
        .credentials
        .delete_api_key()
        .map_err(|_| "Could not remove the API key from the keychain".to_string())?;

    let mut engine = state
        .engine
        .write()
        .map_err(|_| "Engine state lock poisoned".to_string())?;
    *engine = Arc::new(SemanticEngine::from_environment());
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
    let cloud_key_configured = resolve_gemini_key(&KeyringCredentialStore::gemini()).is_some();

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
        .map_err(|_| "Could not store the Gemini key in the keychain".to_string())?;
    Ok(tier2_status())
}

/// Remove the stored Gemini API key.
#[tauri::command]
pub fn clear_tier2_key(state: tauri::State<'_, AppState>) -> Result<Tier2StatusPayload, String> {
    state
        .tier2_credentials
        .delete_api_key()
        .map_err(|_| "Could not remove the Gemini key from the keychain".to_string())?;
    Ok(tier2_status())
}

/// A Tier 2 generation request from the UI.
#[derive(Debug, Clone, Deserialize)]
pub struct Tier2FixRequestDTO {
    /// Workspace root.
    pub root_path: String,
    /// Identifier of a finding from the latest completed scan.
    pub finding_id: String,
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
pub async fn generate_fix(
    state: tauri::State<'_, AppState>,
    mut request: Tier2FixRequestDTO,
) -> Result<PatchPreviewDTO, String> {
    let authorization = state.fix_authorization.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (generation, finding) = {
            let auth = authorization
                .read()
                .map_err(|_| "Fix authorization lock poisoned".to_string())?;
            (
                auth.generation,
                auth.find(
                    &request.root_path,
                    &request.finding_id,
                    &request.file_path,
                    &request.rule_id,
                )?
                .clone(),
            )
        };
        request.title = finding.title.clone();
        request.description = finding.description.clone();
        request.line_start = finding.line_start;
        request.line_end = finding.line_end;
        let source = read_verified_source(&request.root_path, &finding)?;
        let preview = build_tier2_preview(&request, &source)?;
        read_verified_source(&request.root_path, &finding)?;
        authorize_preview(
            &authorization,
            generation,
            &request.root_path,
            &finding,
            &preview,
        )?;
        Ok(preview)
    })
    .await
    .map_err(|error| format!("Tier 2 task join error: {error}"))?
}

fn build_tier2_preview(
    request: &Tier2FixRequestDTO,
    source: &SourceFile,
) -> Result<PatchPreviewDTO, String> {
    let provider: Arc<dyn GenerativeFixProvider> = match request.provider.as_str() {
        "ollama" => Arc::new(OllamaFixProvider::from_env().map_err(|error| error.to_string())?),
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

    let reader: Arc<dyn SourceFileReader> = Arc::new(SnapshotReader(source.clone()));
    let generator = GenerateFix::new(provider, reader);
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
    preview_from_proposal(
        generated.proposal,
        &source.content,
        format!("Tier 2 fix for {}", request.rule_id),
        generated.warnings,
    )
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
    state: tauri::State<'_, AppState>,
    root_path: String,
    finding_id: String,
    file_path: String,
    rule_id: String,
) -> Result<PatchPreviewDTO, String> {
    let authorization = state.fix_authorization.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (generation, finding) = {
            let auth = authorization
                .read()
                .map_err(|_| "Fix authorization lock poisoned".to_string())?;
            (
                auth.generation,
                auth.find(&root_path, &finding_id, &file_path, &rule_id)?
                    .clone(),
            )
        };
        let source = read_verified_source(&root_path, &finding)?;
        let preview = build_preview(&file_path, &rule_id, &source)?;
        read_verified_source(&root_path, &finding)?;
        authorize_preview(&authorization, generation, &root_path, &finding, &preview)?;
        Ok(preview)
    })
    .await
    .map_err(|error| format!("Patch preview task join error: {error}"))?
}

/// Prepare the proposal and render its unified diff.
///
/// A finding without a deterministic fix is not an error: the preview carries
/// no proposal and the modal explains why (`CR-SPEC-13 §5`).
fn build_preview(
    file_path: &str,
    rule_id: &str,
    source: &SourceFile,
) -> Result<PatchPreviewDTO, String> {
    let reader = Arc::new(coderadar_infrastructure::FileSystemSourceReader::new());

    if coderadar_usecases::template_for(rule_id).is_none() {
        return Ok(no_proposal_preview(
            source,
            file_path,
            rule_id,
            format!("No deterministic fix template exists for rule `{rule_id}`."),
            Vec::new(),
        ));
    }

    let prepare = PreparePatch::new(reader);
    let proposal = match block_on(prepare.prepare_from_source(source, file_path, rule_id)) {
        Ok(proposal) => proposal,
        Err(error) => {
            return Ok(no_proposal_preview(
                source,
                file_path,
                rule_id,
                format!(
                "The deterministic template for rule `{rule_id}` does not match this file exactly."
            ),
                vec![error.to_string()],
            ))
        }
    };

    let title = coderadar_usecases::template_for(rule_id)
        .map(|template| template.title.to_string())
        .unwrap_or_else(|| format!("Deterministic fix for {rule_id}"));

    preview_from_proposal(proposal, &source.content, title, Vec::new())
}

fn authorize_preview(
    authorization: &RwLock<FixAuthorization>,
    generation: u64,
    root: &str,
    finding: &AuthorizedFinding,
    preview: &PatchPreviewDTO,
) -> Result<(), String> {
    let mut auth = authorization
        .write()
        .map_err(|_| "Fix authorization lock poisoned".to_string())?;
    if auth.generation != generation
        || auth
            .find(root, &finding.id, &finding.file, &finding.rule_id)?
            .sha256
            != finding.sha256
        || preview.original_sha256 != finding.sha256
    {
        return Err("Fix authorization expired or source changed; run a new scan".into());
    }
    if let Some(proposal) = &preview.proposal {
        auth.issue(generation, root, finding, proposal)?;
    }
    Ok(())
}

/// Render a proposal against the original content as a UI preview.
fn preview_from_proposal(
    proposal: coderadar_domain::PatchProposal,
    original_content: &str,
    title: String,
    warnings: Vec<String>,
) -> Result<PatchPreviewDTO, String> {
    verify_snapshot(&proposal.original_sha256, original_content)?;
    let diff = unified_diff(original_content, &proposal.new_content, &proposal.file_path);
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
        warnings,
        proposal: Some(PatchProposalDTO::from_proposal(&proposal, diff.text)),
    })
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
    state: tauri::State<'_, AppState>,
    root_path: String,
    proposal: PatchProposalDTO,
) -> Result<PatchOutcomeDTO, String> {
    let authorization = state.fix_authorization.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut auth = authorization
            .write()
            .map_err(|_| "Fix authorization lock poisoned".to_string())?;
        let domain_proposal = auth.take_issued(&root_path, &proposal)?.to_proposal();
        let applier = SafePatchApplier::new();
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
        let repository = history.repository().0.clone();
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

#[cfg(test)]
mod fix_safety_tests {
    use super::*;

    #[test]
    fn only_latest_completed_full_scan_authorizes_matching_findings() {
        let mut auth = FixAuthorization::default();
        let old = auth.begin_scan();
        let current = auth.begin_scan();
        let finding = AuthorizedFinding {
            id: "id".into(),
            file: "a".into(),
            rule_id: "rule".into(),
            title: "title".into(),
            description: "description".into(),
            line_start: 1,
            line_end: 2,
            sha256: "hash".into(),
        };
        auth.finish_scan(old, "/root".into(), true, vec![finding.clone()]);
        assert!(auth.find("/root", "id", "a", "rule").is_err());
        auth.finish_scan(current, "/root".into(), true, vec![finding.clone()]);
        assert!(auth.find("/root", "id", "a", "rule").is_ok());
        assert!(auth.find("/other", "id", "a", "rule").is_err());
        assert!(auth.find("/root", "other", "a", "rule").is_err());
        assert!(auth.find("/root", "id", "b", "rule").is_err());
        let staged = auth.begin_scan();
        assert!(auth.find("/root", "id", "a", "rule").is_err());
        auth.finish_scan(staged, "/root".into(), false, vec![finding]);
        assert!(auth.find("/root", "id", "a", "rule").is_err());
    }

    #[test]
    fn unrecognized_fix_requests_are_refused_without_a_scan() {
        assert!(FixAuthorization::default()
            .find("missing", "id", "file", "rule")
            .is_err());
    }

    #[test]
    fn superseded_scan_cannot_publish_and_multiple_previews_remain_issued() {
        let mut auth = FixAuthorization::default();
        let old = auth.begin_scan();
        let generation = auth.begin_scan();
        assert!(!auth.is_current(old));
        assert!(auth.is_current(generation));
        let finding = AuthorizedFinding {
            id: "id".into(),
            file: "a".into(),
            rule_id: "rule".into(),
            title: "title".into(),
            description: "description".into(),
            line_start: 1,
            line_end: 1,
            sha256: "hash".into(),
        };
        auth.finish_scan(generation, "root".into(), true, vec![finding.clone()]);
        let proposal =
            coderadar_domain::PatchProposal::new("a", "rule", "hash", 1, vec![], "first", "reason");
        let first = PatchProposalDTO::from_proposal(&proposal, "first".into());
        let mut second = first.clone();
        second.new_content = "second".into();
        auth.issue(generation, "root", &finding, &first).unwrap();
        auth.issue(generation, "root", &finding, &second).unwrap();
        assert!(auth.issued("root", &first).is_ok());
        assert!(auth.issued("root", &second).is_ok());
        auth.take_issued("root", &first).unwrap();
        assert!(auth.issued("root", &first).is_err());
        assert!(auth.issued("root", &second).is_ok());
    }

    #[test]
    fn snapshot_and_issued_proposals_are_bound_to_generation() {
        let mut auth = FixAuthorization::default();
        let generation = auth.begin_scan();
        let finding = AuthorizedFinding {
            id: "id".into(),
            file: "a".into(),
            rule_id: "rule".into(),
            title: "title".into(),
            description: "description".into(),
            line_start: 1,
            line_end: 1,
            sha256: coderadar_infrastructure::sha256_hex("old"),
        };
        auth.finish_scan(generation, "root".into(), true, vec![finding.clone()]);
        assert!(verify_snapshot(&finding.sha256, "changed").is_err());
        assert!(verify_snapshot(&finding.sha256, "old").is_ok());
        let proposal = coderadar_domain::PatchProposal::new(
            "a",
            "rule",
            finding.sha256.clone(),
            1,
            vec![],
            "new",
            "reason",
        );
        let dto = PatchProposalDTO::from_proposal(&proposal, "diff".into());
        assert!(
            preview_from_proposal(proposal.clone(), "changed", "title".into(), vec![]).is_err()
        );
        assert!(preview_from_proposal(proposal, "old", "title".into(), vec![]).is_ok());
        let mut wrong_hash = dto.clone();
        wrong_hash.original_sha256 = "other".into();
        assert!(auth
            .issue(generation, "root", &finding, &wrong_hash)
            .is_err());
        assert!(auth.issue(generation, "root", &finding, &dto).is_ok());
        assert!(auth.issued("root", &dto).is_ok());
        assert!(auth.issued("other", &dto).is_err());
        let mut forged = dto.clone();
        forged.new_content = "forged".into();
        assert!(auth.issued("root", &forged).is_err());
        auth.begin_scan();
        assert!(auth.issued("root", &dto).is_err());
        assert!(auth.issue(generation, "root", &finding, &dto).is_err());
    }
}
