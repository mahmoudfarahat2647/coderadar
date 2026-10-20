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

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use coderadar_domain::DomainError;
use coderadar_usecases::{ScanHistoryRepository, SemanticRuleEvaluator};

use crate::executor::block_on;
use crate::layer_map::LayerMap;
use crate::mock::MockRuleEvaluator;
use crate::pipeline::{ScanOutcome, ScanPipeline};
use crate::report::ScanReport;
use crate::router::RuleEngineRouter;
use crate::rules_manifest::RuleManifest;
use crate::throttler::{FindingBatch, FindingThrottler};

/// Run a full scan of `root` over the offline Mock engine.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project(root: &Path) -> Result<ScanReport, DomainError> {
    scan_project_with(root, Arc::new(MockRuleEvaluator::new()))
}

/// Run a full scan of `root` over an injected semantic engine.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project_with(
    root: &Path,
    semantic: Arc<dyn SemanticRuleEvaluator>,
) -> Result<ScanReport, DomainError> {
    let manifest = RuleManifest::embedded()?;
    let layer_map = LayerMap::load(root);
    let source = layer_map.source();
    let router = RuleEngineRouter::new(Arc::new(layer_map)).with_semantic(semantic);
    let pipeline = ScanPipeline::new(Arc::new(router), manifest.rules.clone());
    let started = Instant::now();
    let outcome = pipeline.run(root)?;
    let duration_ms = started.elapsed().as_millis() as u64;
    Ok(ScanReport::from_scan(
        &outcome,
        &manifest,
        source,
        duration_ms,
    ))
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
    scan_project_streaming_with(
        root,
        Arc::new(MockRuleEvaluator::new()),
        flush_interval,
        on_batch,
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
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    run_streaming(root, semantic, flush_interval, on_batch).map(|(_, report)| report)
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
) -> Result<ScanReport, DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    let (outcome, report) = run_streaming(root, semantic, flush_interval, on_batch)?;
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
    on_batch: F,
) -> Result<(ScanOutcome, ScanReport), DomainError>
where
    F: FnMut(FindingBatch) + Send + 'static,
{
    let manifest = RuleManifest::embedded()?;
    let layer_map = LayerMap::load(root);
    let source = layer_map.source();
    let router = RuleEngineRouter::new(Arc::new(layer_map)).with_semantic(semantic);
    let pipeline = ScanPipeline::new(Arc::new(router), manifest.rules.clone());
    let started = Instant::now();

    let (sender, handle) = FindingThrottler::start(flush_interval, on_batch);
    let sender_clone = sender.clone();
    let outcome = pipeline.run_with_callback(root, move |findings| {
        sender_clone.send_batch(findings);
    })?;
    drop(sender);
    handle.join();

    let duration_ms = started.elapsed().as_millis() as u64;
    let report = ScanReport::from_scan(&outcome, &manifest, source, duration_ms);
    Ok((outcome, report))
}
