//! CodeRadar Studio — Layer 2: Application Business Rules (Use Cases).
//!
//! This layer orchestrates the domain entities and communicates with the
//! outside world **only** through vendor-agnostic ports. Invariants enforced by
//! CI (see `scripts/check-architecture.sh`):
//!
//! * the only allowed dependency is `coderadar-domain`;
//! * no `tokio` (or any async runtime), no `reqwest`, no `sqlx`, no outer
//!   adapter or infrastructure crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod fixes;
mod ports;

pub use fixes::{template_for, templated_rule_ids, FixTemplate, PreparePatch, ALL_TEMPLATES};
pub use ports::{
    BoxFuture, ChunkSink, PatchApplier, ProjectScannerGateway, ScanHistoryRepository,
    SemanticRuleEvaluator, SourceFile, SourceFileReader,
};
