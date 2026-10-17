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
use coderadar_usecases::SemanticRuleEvaluator;

use crate::layer_map::LayerMap;
use crate::mock::MockRuleEvaluator;
use crate::pipeline::ScanPipeline;
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
    Ok(ScanReport::from_scan(
        &outcome,
        &manifest,
        source,
        duration_ms,
    ))
}
