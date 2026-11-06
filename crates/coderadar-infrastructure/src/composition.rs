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

use coderadar_domain::{DomainError, ScanMode, ScanUsage};
use coderadar_usecases::{ScanHistoryRepository, SemanticRuleEvaluator};

use crate::engine::SemanticEngine;
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

/// Run a full scan with a fresh per-scan usage scope.
///
/// # Errors
/// Returns an error if the scan cannot be completed.
pub fn scan_project_with_engine(
    root: &Path,
    engine: &SemanticEngine,
) -> Result<ScanReport, DomainError> {
    scan_project_with_engine_mode(root, engine, ScanMode::FullScan)
}

/// Run a scan with a fresh per-scan usage scope, in the selected mode.
///
/// # Errors
/// Returns an error if the scan cannot be completed.
pub fn scan_project_with_engine_mode(
    root: &Path,
    engine: &SemanticEngine,
    mode: ScanMode,
) -> Result<ScanReport, DomainError> {
    match engine.scoped_evaluator() {
        Some((evaluator, usage)) => run_batch(root, evaluator, mode, move || {
            let snapshot = usage.snapshot();
            (
                snapshot.legacy_engine_mode().to_owned(),
                Some(snapshot.into()),
            )
        })
        .map(|(_, report)| report),
        None => scan_project_with_mode(root, engine.evaluator(), mode, mock_engine),
    }
}

/// Stream and persist a scan with a fresh per-scan usage scope.
///
/// # Errors
/// Returns an error if the scan cannot be completed; persistence errors are logged.
pub fn scan_project_streaming_with_engine_history_mode<F>(
    root: &Path,
    engine: &SemanticEngine,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    history: &dyn ScanHistoryRepository,
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    scan_project_streaming_with_engine_history_status(
        root,
        engine,
        flush_interval,
        mode,
        on_batch,
        history,
    )
    .map(|(report, _)| report)
}

/// Stream a scan and return whether its history write succeeded separately.
///
/// # Errors
/// Returns an error only when the scan cannot be completed.
pub fn scan_project_streaming_with_engine_history_status<F>(
    root: &Path,
    engine: &SemanticEngine,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    history: &dyn ScanHistoryRepository,
) -> Result<(ScanReport, bool), DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    scan_engine_history_status(root, engine, flush_interval, mode, on_batch, history, false)
}

/// Stream provisional findings separately from the authoritative completed report.
///
/// # Errors
/// Returns an error if the scan cannot be completed.
pub fn scan_project_provisional_with_engine_history_status<F>(
    root: &Path,
    engine: &SemanticEngine,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    history: &dyn ScanHistoryRepository,
) -> Result<(ScanReport, bool), DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    scan_engine_history_status(root, engine, flush_interval, mode, on_batch, history, true)
}

fn scan_engine_history_status<F>(
    root: &Path,
    engine: &SemanticEngine,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    history: &dyn ScanHistoryRepository,
    provisional: bool,
) -> Result<(ScanReport, bool), DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    match engine.scoped_evaluator() {
        Some((evaluator, usage)) => {
            let (outcome, report) = run_streaming_inner(
                root,
                evaluator,
                flush_interval,
                mode,
                on_batch,
                provisional,
                move || {
                    let snapshot = usage.snapshot();
                    (
                        snapshot.legacy_engine_mode().to_owned(),
                        Some(snapshot.into()),
                    )
                },
            )?;
            Ok((report, save_history(history, &outcome)))
        }
        None => {
            let (outcome, report) = run_streaming_inner(
                root,
                engine.evaluator(),
                flush_interval,
                mode,
                on_batch,
                provisional,
                || (mock_engine(), None),
            )?;
            Ok((report, save_history(history, &outcome)))
        }
    }
}

fn save_history(history: &dyn ScanHistoryRepository, outcome: &ScanOutcome) -> bool {
    match block_on(history.save_scan_session(&outcome.session)) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("[coderadar] the scan session cannot be persisted: {error}");
            false
        }
    }
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
    run_batch(root, semantic, mode, || (engine_mode(), None)).map(|(_, report)| report)
}

/// Shared batched implementation: one outcome plus its report.
fn run_batch(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    mode: ScanMode,
    engine_mode: impl Fn() -> (String, Option<ScanUsage>) + Send + Sync,
) -> Result<(ScanOutcome, ScanReport), DomainError> {
    let active = RulesStore::new(RulesStore::default_data_dir()).active_or_embedded();
    let manifest = active.manifest;
    let snapshot = if mode == ScanMode::StagedDiff {
        Some(Arc::new(GitChangesetReader::capture_staged(root)?))
    } else {
        None
    };
    let (layer_map, configuration) = match &snapshot {
        Some(snapshot) => (LayerMap::load_staged(snapshot)?, None),
        None => {
            let (map, identity) = LayerMap::capture_full(root);
            (map, Some(identity))
        }
    };
    let source = layer_map.source();
    let router = RuleEngineRouter::new(Arc::new(layer_map)).with_semantic(semantic);
    let mut pipeline =
        build_pipeline(Arc::new(router), &manifest)?.with_configuration_fingerprint(configuration);
    if let Some(snapshot) = snapshot {
        pipeline = pipeline.with_staged_snapshot(snapshot);
    }
    let started = Instant::now();

    let mut outcome = pipeline.run_mode(root, mode)?;
    // Label the session only after the scan: a fallback recorded during the
    // run must win over the pre-scan engine status (`CR-SPEC-12 §4`).
    let (mode, usage) = engine_mode();
    outcome.session = outcome.session.with_engine_mode(mode);
    outcome.session.usage = usage;
    let duration_ms = started.elapsed().as_millis() as u64;
    let report = ScanReport::from_scan(&outcome, &manifest, source, duration_ms);
    Ok((outcome, report))
}

/// Build the pipeline with the Git changeset gateway every scan needs.
fn build_pipeline(
    evaluator: Arc<dyn coderadar_usecases::SemanticRuleEvaluator>,
    manifest: &RuleManifest,
) -> Result<ScanPipeline, DomainError> {
    RuleEngineRouter::validate_ownership(&manifest.rules)?;
    Ok(ScanPipeline::new(evaluator, manifest.rules.clone())
        .with_changeset_gateway(Arc::new(GitChangesetReader::new())))
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
    run_streaming(root, semantic, flush_interval, mode, on_batch, || {
        (engine_mode(), None)
    })
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
    let (outcome, report) = run_streaming(root, semantic, flush_interval, mode, on_batch, || {
        (engine_mode(), None)
    })?;
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
    engine_mode: impl Fn() -> (String, Option<ScanUsage>) + Send + Sync,
) -> Result<(ScanOutcome, ScanReport), DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    run_streaming_inner(
        root,
        semantic,
        flush_interval,
        mode,
        on_batch,
        false,
        engine_mode,
    )
}

fn run_streaming_inner<F>(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
    flush_interval: Duration,
    mode: ScanMode,
    on_batch: F,
    provisional: bool,
    engine_mode: impl Fn() -> (String, Option<ScanUsage>) + Send + Sync,
) -> Result<(ScanOutcome, ScanReport), DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    let active = RulesStore::new(RulesStore::default_data_dir()).active_or_embedded();
    let manifest = active.manifest;
    let snapshot = if mode == ScanMode::StagedDiff {
        Some(Arc::new(GitChangesetReader::capture_staged(root)?))
    } else {
        None
    };
    let (layer_map, configuration) = match &snapshot {
        Some(snapshot) => (LayerMap::load_staged(snapshot)?, None),
        None => {
            let (map, identity) = LayerMap::capture_full(root);
            (map, Some(identity))
        }
    };
    let source = layer_map.source();
    let router = RuleEngineRouter::new(Arc::new(layer_map)).with_semantic(semantic);
    let mut pipeline =
        build_pipeline(Arc::new(router), &manifest)?.with_configuration_fingerprint(configuration);
    if let Some(snapshot) = snapshot {
        pipeline = pipeline.with_staged_snapshot(snapshot);
    }
    let started = Instant::now();

    let (sender, handle) = FindingThrottler::start(flush_interval, on_batch);
    let sender_clone = sender.clone();
    let outcome = if provisional {
        pipeline.run_mode_with_provisional_callback(root, mode, move |findings| {
            sender_clone.send_batch(findings)
        })
    } else {
        pipeline.run_mode_with_callback(root, mode, move |findings| {
            sender_clone.send_batch(findings)
        })
    };
    drop(sender);
    handle.join();
    let outcome = outcome?;

    // Label the session only after the throttler has been drained: a fallback
    // recorded during the run must win over the pre-scan engine status
    // (`CR-SPEC-12 §4`).
    let (mode, usage) = engine_mode();
    let mut session = outcome.session.with_engine_mode(mode);
    session.usage = usage;
    let outcome = ScanOutcome { session, ..outcome };
    let duration_ms = started.elapsed().as_millis() as u64;
    let report = ScanReport::from_scan(&outcome, &manifest, source, duration_ms);
    Ok((outcome, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_joins_worker_when_scan_fails() {
        let dir = tempfile::tempdir().unwrap();
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&finished);
        let result = run_streaming(
            &dir.path().join("missing"),
            Arc::new(MockRuleEvaluator::new()),
            Duration::from_millis(5),
            ScanMode::FullScan,
            move |_| {
                std::thread::sleep(Duration::from_millis(30));
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            },
            || (mock_engine(), None),
        );
        assert!(result.is_err());
        assert!(finished.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn streaming_report_and_history_share_scoped_usage() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("example.ts"), "export const x = 1;\n").unwrap();
        let (evaluator, usage) = crate::engine::FallbackRuleEvaluator::new_scoped(
            Arc::new(MockRuleEvaluator::new()),
            Arc::new(MockRuleEvaluator::new()),
            Arc::new(crate::engine::EngineStatus::new(
                crate::engine::EngineMode::Tier1Untested,
            )),
        );
        let repo = crate::history::SqliteScanHistoryRepository::open_in_memory().unwrap();
        let (outcome, report) = run_streaming(
            dir.path(),
            Arc::new(evaluator),
            Duration::from_millis(1),
            ScanMode::FullScan,
            |_| {},
            || {
                let snapshot = usage.snapshot();
                (
                    snapshot.legacy_engine_mode().to_owned(),
                    Some(snapshot.into()),
                )
            },
        )
        .unwrap();
        block_on(repo.save_scan_session(&outcome.session)).unwrap();
        let history = block_on(repo.get_history(10, None)).unwrap();
        let json = serde_json::to_value(report).unwrap();
        let recorded = history[0].usage.unwrap();
        assert!(recorded.primary_batches > 0);
        assert_eq!(
            json["session"]["usage"]["primary_batches"],
            recorded.primary_batches
        );
        assert_eq!(json["session"]["usage"]["fallback_batches"], 0);
        assert_eq!(json["session"]["usage"]["fallback_failures"], 0);
    }

    #[test]
    fn pipeline_rejects_invalid_ownership_before_scanning() {
        let mut manifest = RuleManifest::embedded().unwrap();
        manifest.rules[0].id = "UNKNOWN-RULE".to_string();
        assert!(build_pipeline(Arc::new(MockRuleEvaluator::new()), &manifest).is_err());
    }
}
