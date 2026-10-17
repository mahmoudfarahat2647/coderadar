//! Composition root — wiring Layer 4 drivers into a runnable scan.
//!
//! This is the only place that knows the concrete engine graph:
//! [`RuleEngineRouter`] over the loaded [`LayerMap`], driven by
//! [`ScanPipeline`] with the embedded [`RuleManifest`]. Both the CLI and (from
//! Phase 3 on) the Tauri command layer call into this module.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use coderadar_domain::DomainError;

use crate::layer_map::LayerMap;
use crate::pipeline::ScanPipeline;
use crate::report::ScanReport;
use crate::router::RuleEngineRouter;
use crate::rules_manifest::RuleManifest;

/// Run a full scan of `root` and produce the serializable report.
///
/// # Errors
/// Returns [`DomainError`] when the root path is invalid, the manifest fails
/// to load, or the scan pipeline aborts.
pub fn scan_project(root: &Path) -> Result<ScanReport, DomainError> {
    let manifest = RuleManifest::embedded()?;
    let layer_map = LayerMap::load(root);
    let source = layer_map.source();
    let router = RuleEngineRouter::new(Arc::new(layer_map));
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
