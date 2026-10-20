//! Vendor-agnostic boundary ports (Rust traits).
//!
//! Every port is defined in this layer and implemented by an outer layer
//! (adapters or infrastructure) following the Dependency Inversion Principle.
//! Asynchronous boundaries are expressed with plain `std::future::Future`
//! trait objects, so this crate never links an async runtime.

use std::future::Future;
use std::pin::Pin;

use coderadar_domain::{
    CodeChunk, DomainError, PatchOutcome, PatchProposal, RuleEvaluationResult, RuleSpec, ScanMode,
    ScanSession, ScanSessionSummary,
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
    ///
    /// When `root` is given, only the sessions of that workspace are returned.
    /// The workspace may be spelled in any way the caller knows it: the
    /// implementation canonicalises it exactly like the stored session.
    fn get_history<'a>(
        &'a self,
        limit: usize,
        root: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<ScanSessionSummary>, DomainError>>;
}

/// A workspace file together with its content fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Full text of the file.
    pub content: String,
    /// SHA-256 (hex) of the content.
    pub sha256: String,
    /// Number of lines in the content.
    pub line_count: usize,
}

/// Reads a file from the workspace and fingerprints it.
///
/// The reader is the only way the use cases get access to file content; the
/// path handling (traversal and symlink guards) lives in the implementation.
pub trait SourceFileReader: Send + Sync {
    /// Read `relative_path` inside `root`.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the path escapes the workspace, the file
    /// does not exist, or it cannot be read.
    fn read_source<'a>(
        &'a self,
        root: &'a str,
        relative_path: &'a str,
    ) -> BoxFuture<'a, Result<SourceFile, DomainError>>;
}

/// Applies a [`PatchProposal`] to the workspace.
pub trait PatchApplier: Send + Sync {
    /// Apply `proposal` inside `root`.
    ///
    /// Implementations must refuse to write when the file on disk no longer
    /// matches `proposal.original_sha256`, and must write atomically.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the workspace cannot be reached at all.
    /// A *refused* patch is reported as [`PatchOutcome`], not as an error.
    fn apply_patch<'a>(
        &'a self,
        root: &'a str,
        proposal: &'a PatchProposal,
    ) -> BoxFuture<'a, Result<PatchOutcome, DomainError>>;
}
