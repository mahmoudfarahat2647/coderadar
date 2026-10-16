//! Vendor-agnostic boundary ports (Rust traits).
//!
//! Every port is defined in this layer and implemented by an outer layer
//! (adapters or infrastructure) following the Dependency Inversion Principle.
//! Asynchronous boundaries are expressed with plain `std::future::Future`
//! trait objects, so this crate never links an async runtime.

use std::future::Future;
use std::pin::Pin;

use coderadar_domain::{
    CodeChunk, DomainError, RuleEvaluationResult, RuleSpec, ScanMode, ScanSession,
    ScanSessionSummary,
};

/// A boxed, `Send` future returned across an asynchronous port boundary.
///
/// Using a plain `std` future keeps Layer 2 free of any async-runtime
/// dependency (`tokio`, `async-std`, ...).
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Evaluates a batch of code chunks against a set of rules.
///
/// Implemented by the Mock heuristic engine, the Tier 0 static analyser and the
/// Jev Tier 1 semantic engine. All three are 100% interchangeable
/// (Liskov substitution, `CR-SPEC-02 §5`).
pub trait SemanticRuleEvaluator: Send + Sync {
    /// Evaluate `rules` against `chunks`, returning one result per rule.
    fn evaluate_batch<'a>(
        &'a self,
        chunks: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>>;
}

/// Receives code chunks streamed by a [`ProjectScannerGateway`].
///
/// This decouples the scanner from any concrete channel, queue, or runtime.
pub trait ChunkSink: Send + Sync {
    /// Consume a single chunk.
    ///
    /// # Errors
    /// Returns [`DomainError`] if the sink can no longer accept chunks.
    fn send_chunk(&self, chunk: CodeChunk) -> Result<(), DomainError>;
}

/// Streams the code chunks of a project into a [`ChunkSink`].
pub trait ProjectScannerGateway: Send + Sync {
    /// Walk `root_dir` according to `mode` and push every chunk into `sink`.
    ///
    /// # Errors
    /// Returns [`DomainError`] if the project cannot be walked or read.
    fn stream_project_chunks(
        &self,
        root_dir: &str,
        mode: ScanMode,
        sink: &(dyn ChunkSink + 'static),
    ) -> Result<(), DomainError>;
}

/// Persists and retrieves scan history.
pub trait ScanHistoryRepository: Send + Sync {
    /// Persist a completed scan session.
    fn save_scan_session<'a>(
        &'a self,
        session: &'a ScanSession,
    ) -> BoxFuture<'a, Result<(), DomainError>>;

    /// Load the most recent scan summaries, newest first.
    fn get_history<'a>(
        &'a self,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<ScanSessionSummary>, DomainError>>;
}
