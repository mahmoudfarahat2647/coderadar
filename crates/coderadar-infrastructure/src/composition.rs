//! Composition root — wiring Layer 4 drivers into a runnable scan.
//!
//! This is the only place that knows the concrete engine graph:
//! [`RuleEngineRouter`] over the loaded [`LayerMap`], driven by
//! [`ScanPipeline`] with the embedded [`RuleManifest`]. Both the CLI and the
//! Tauri command layer call into this module.
//!
//! The semantic engine is injected so a scan can run over the offline Mock
//! engine, the live Jev adapter, or the Jev-with-Mock-fallback decorator
//! (`CR-SPEC-12 §4`) without any use case knowing which one is active.
//!
//! Every entry point that accepts an injected evaluator takes an engine-mode
//! label along with it — a lazily evaluated `Fn() -> String` that is read once
//! the pipeline has finished, before the report is rendered and the session is
//! persisted. The laziness is the point: a Jev batch that falls back to Mock
//! mid-scan is recorded as `mock_fallback` (`CR-SPEC-12 §4`). Entry points
//! that construct the [`MockRuleEvaluator`] themselves hardwire the `mock`
//! label instead.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use coderadar_domain::{DomainError, ScanMode};
use coderadar_usecases::{ScanHistoryRepository, SemanticRuleEvaluator};

use crate::executor::block_on;
use crate::git_changeset::GitChangesetReader;
use crate::layer_map::LayerMap;
use crate::mock::MockRuleEvaluator;
use crate::pipeline::{ScanOutcome, ScanPipeline};
use crate::report::ScanReport;
use crate::router::RuleEngineRouter;
use crate::rules_manifest::RuleManifest;
use crate::rules_ota::RulesStore;
use crate::throttler::{FindingBatch, FindingThrottler};

/// Engine label of the offline compositions: the CLI only ever drives the
/// Mock engine (`AGENTS.md`, findings precision).
fn mock_engine() -> String {
    "mock".to_string()
}

/// Run a full scan of `root` over the offline Mock engine.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project(root: &Path) -> Result<ScanReport, DomainError> {
    scan_project_with_mode(
        root,
        Arc::new(MockRuleEvaluator::new()),
        ScanMode::FullScan,
        mock_engine,
    )
}

/// Run a scan of `root` in `mode` over the offline Mock engine.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project_mode(root: &Path, mode: ScanMode) -> Result<ScanReport, DomainError> {
    scan_project_with_mode(root, Arc::new(MockRuleEvaluator::new()), mode, mock_engine)
}

/// Run a full scan of `root` over an injected semantic engine.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project_with(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<ScanReport, DomainError> {
    scan_project_with_mode(root, semantic, ScanMode::FullScan, engine_mode)
}

/// Run a scan of `root` in `mode` over an injected semantic engine.
///
/// `engine_mode` is a lazy engine label (`mock` | `jev` | `mock_fallback`);
/// it is read once the pipeline has finished, so the label reflects any
/// fallback that happened during the scan.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project_with_mode(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    mode: ScanMode,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<ScanReport, DomainError> {
    run_batch(root, semantic, mode, engine_mode).map(|(_, report)| report)
}

/// Shared batched implementation: one outcome plus its report.
fn run_batch(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    mode: ScanMode,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<(ScanOutcome, ScanReport), DomainError> {
    let active = RulesStore::new(RulesStore::default_data_dir()).active_or_embedded();
    let manifest = active.manifest;
    let layer_map = LayerMap::load(root);
    let source = layer_map.source();
    let router = RuleEngineRouter::new(Arc::new(layer_map)).with_semantic(semantic);
    let pipeline = build_pipeline(Arc::new(router), &manifest);
    let started = Instant::now();

    let mut outcome = pipeline.run_mode(root, mode)?;
    // Label the session only after the scan: a fallback recorded during the
    // run must win over the pre-scan engine status (`CR-SPEC-12 §4`).
    outcome.session = outcome.session.with_engine_mode(engine_mode());
    let duration_ms = started.elapsed().as_millis() as u64;
    let report = ScanReport::from_scan(&outcome, &manifest, source, duration_ms);
    Ok((outcome, report))
}

/// Build the pipeline with the Git changeset gateway every scan needs.
fn build_pipeline(
    evaluator: Arc<dyn coderadar_usecases::SemanticRuleEvaluator>,
    manifest: &RuleManifest,
) -> ScanPipeline {
    ScanPipeline::new(evaluator, manifest.rules.clone())
        .with_changeset_gateway(Arc::new(GitChangesetReader::new()))
}

/// Run a full scan of `root` over the offline Mock engine, streaming batched
/// findings to `on_batch` via [`FindingThrottler`] at `flush_interval`.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project_streaming<F>(
    root: &Path,
    flush_interval: Duration,
    on_batch: F,
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    scan_project_streaming_with_mode(
        root,
        Arc::new(MockRuleEvaluator::new()),
        flush_interval,
        ScanMode::FullScan,
        on_batch,
        mock_engine,
    )
}

/// Run a full scan of `root` over an injected semantic engine, streaming
/// batched findings to `on_batch`.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project_streaming_with<F>(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    flush_interval: Duration,
    on_batch: F,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    scan_project_streaming_with_mode(
        root,
        semantic,
        flush_interval,
        ScanMode::FullScan,
        on_batch,
        engine_mode,
    )
}

/// Run a scan of `root` in `mode` over an injected semantic engine, streaming
/// batched findings to `on_batch`.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project_streaming_with_mode<F>(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    run_streaming(root, semantic, flush_interval, mode, on_batch, engine_mode)
        .map(|(_, report)| report)
}

/// Run a full scan of `root` and persist the finished session in `history`.
///
/// The scan itself is unaffected by the history repository: the session is
/// snapshotted once the pipeline has finished (`CR-SPEC-13 §6`). A
/// persistence failure is logged and does **not** invalidate the scan.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts. A session that cannot be persisted
/// is only logged: the report of the finished scan is still returned.
pub fn scan_project_streaming_with_history<F>(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    flush_interval: Duration,
    on_batch: F,
    history: &dyn ScanHistoryRepository,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    scan_project_streaming_with_history_mode(
        root,
        semantic,
        flush_interval,
        ScanMode::FullScan,
        on_batch,
        history,
        engine_mode,
    )
}

/// Run a scan of `root` in `mode` and persist the finished session in `history`.
///
/// `engine_mode` is a lazy engine label (`mock` | `jev` | `mock_fallback`)
/// recorded on the session and in the history row. It is read after the
/// pipeline has finished and before the session row is written, so a fallback
/// that happens mid-scan is recorded as `mock_fallback` (`CR-SPEC-12 §4`).
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts. A session that cannot be persisted
/// is only logged: the report of the finished scan is still returned.
pub fn scan_project_streaming_with_history_mode<F>(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    history: &dyn ScanHistoryRepository,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    let (outcome, report) =
        run_streaming(root, semantic, flush_interval, mode, on_batch, engine_mode)?;
    if let Err(error) = block_on(history.save_scan_session(&outcome.session)) {
        eprintln!("[coderadar] the scan session cannot be persisted: {error}");
    }
    Ok(report)
}

/// Shared streaming implementation used by the public entry points.
fn run_streaming<F>(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    engine_mode: impl Fn() -> String + Send + Sync,
) -> Result<(ScanOutcome, ScanReport), DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    let active = RulesStore::new(RulesStore::default_data_dir()).active_or_embedded();
    let manifest = active.manifest;
    let layer_map = LayerMap::load(root);
    let source = layer_map.source();
    let router = RuleEngineRouter::new(Arc::new(layer_map)).with_semantic(semantic);
    let pipeline = build_pipeline(Arc::new(router), &manifest);
    let started = Instant::now();

    let (sender, handle) = FindingThrottler::start(flush_interval, on_batch);
    let sender_clone = sender.clone();
    let outcome = pipeline.run_mode_with_callback(root, mode, move |findings| {
        sender_clone.send_batch(findings);
    })?;
    drop(sender);
    handle.join();

    // Label the session only after the throttler has been drained: a fallback
    // recorded during the run must win over the pre-scan engine status
    // (`CR-SPEC-12 §4`).
    let session = outcome.session.with_engine_mode(engine_mode());
    let outcome = ScanOutcome { session, ..outcome };
    let duration_ms = started.elapsed().as_millis() as u64;
    let report = ScanReport::from_scan(&outcome, &manifest, source, duration_ms);
    Ok((outcome, report))
}
