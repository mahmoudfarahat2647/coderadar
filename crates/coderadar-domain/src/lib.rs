//! CodeRadar Studio — Layer 1: Domain Core.
//!
//! The innermost circle of the Clean Architecture onion (see `CR-SPEC-02 §2`).
//! It models the enterprise business rules and depends on **`std` only**: no
//! external crate, no serialization library, no async runtime, no database
//! driver. Everything here is pure and deterministic and therefore trivially
//! testable without a UI, network, or filesystem.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod category;
mod changeset;
mod chunk;
mod error;
mod finding;
mod patch;
mod rule;
mod scan;
mod score;
mod severity;

pub use category::FindingCategory;
pub use changeset::{
    is_code_path, is_public_api_removal, is_test_path, Changeset, FileChange, PrViolation,
    PullRequestHygiene, SCOPE_CREEP_AREA_THRESHOLD, SCOPE_CREEP_LINE_THRESHOLD,
};
pub use chunk::{ChunkKind, CodeChunk, RuleEvaluationResult};
pub use error::DomainError;
pub use finding::Finding;
pub use patch::{apply_edits, LineEdit, PatchOutcome, PatchProposal};
pub use rule::{EvaluationEngine, RuleScope, RuleSpec};
pub use scan::{ScanMode, ScanSession, ScanSessionSummary};
pub use score::{QualityScore, RiskScore};
pub use severity::Severity;
