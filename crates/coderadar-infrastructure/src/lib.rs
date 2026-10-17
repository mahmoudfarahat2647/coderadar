//! CodeRadar Studio — Layer 4: Frameworks & Drivers.
//!
//! The outermost circle. Concrete drivers live here: the Tauri webview window,
//! the parallel file scanner, the Jev HTTP client, the SQLite repository and
//! the rules-manifest loader. This is the only layer allowed to depend on
//! external crates such as `serde`, `tokio`, `reqwest` or `sqlx`, and it may
//! depend inward on every other layer.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod chunker;
mod composition;
mod executor;
mod heuristics;
mod layer_map;
mod mock;
mod pipeline;
mod report;
mod router;
mod rules_manifest;
mod scanner;
mod sinks;
mod tier0;

pub use chunker::{detect_language, Chunker, JAVASCRIPT, TYPESCRIPT};
pub use composition::scan_project;
pub use layer_map::{LayerMap, LayerMapSource, ProjectLayer};
pub use mock::MockRuleEvaluator;
pub use pipeline::{ScanOutcome, ScanPipeline};
pub use report::{
    FindingReport, ScanReport, ScanStatsReport, SessionReport, SeverityTotals, ToolInfo,
};
pub use router::RuleEngineRouter;
pub use rules_manifest::RuleManifest;
pub use scanner::{ParallelFileScanner, MAX_FILE_BYTES, MAX_LINE_LENGTH};
pub use sinks::CollectingChunkSink;
pub use tier0::Tier0RuleEvaluator;
