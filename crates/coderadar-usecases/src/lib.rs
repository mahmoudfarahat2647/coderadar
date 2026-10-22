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
mod pull_request;
#[cfg(test)]
mod test_support;
mod tier2;

pub use fixes::{template_for, templated_rule_ids, FixTemplate, PreparePatch, ALL_TEMPLATES};
pub use ports::{
    BoxFuture, ChangesetGateway, ChunkSink, GenerativeFixProvider, PatchApplier,
    ProjectScannerGateway, ScanHistoryRepository, SemanticRuleEvaluator, SourceFile,
    SourceFileReader,
};
pub use pull_request::{AssessPullRequest, PullRequestAssessment};
pub use tier2::{
    FixFinding, FixGenerationRequest, GenerateFix, GeneratedFix, GeneratedFixDraft,
    MAX_INPUT_BYTES, MAX_OUTPUT_BYTES,
};
