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
mod engine;
mod engine_rules;
mod executor;
mod heuristics;
mod jev;
mod keychain;
mod layer_map;
mod mock;
mod pipeline;
mod redaction;
mod report;
mod router;
mod rules_manifest;
mod scanner;
mod sinks;
mod throttler;
mod tier0;

pub use chunker::{detect_language, Chunker, JAVASCRIPT, TYPESCRIPT};
pub use composition::{
    scan_project, scan_project_streaming, scan_project_streaming_with, scan_project_with,
};
pub use engine::{
    EngineMode, EngineStatus, EngineStatusSnapshot, FallbackRuleEvaluator, SemanticEngine,
};
pub use engine_rules::{is_semantic_rule, GIT_CHANGESET_IDS, TIER0_OWNED_IDS};
pub use jev::{
    HttpJevTransport, JevAnswer, JevConfig, JevQuestion, JevRequest, JevResponse,
    JevRuleEvaluatorAdapter, JevTransport, JevTransportError, DEFAULT_BATCH_SIZE,
    DEFAULT_CONFIDENCE_THRESHOLD, DEFAULT_ENDPOINT, DEFAULT_MAX_PARALLEL, DEFAULT_MODEL,
    DEFAULT_TIMEOUT_MS,
};
pub use keychain::{
    mask_api_key, resolve_api_key, CredentialStore, InMemoryCredentialStore,
    KeyringCredentialStore, JEV_API_KEY_ENV, KEYRING_ACCOUNT, KEYRING_SERVICE,
};
pub use layer_map::{LayerMap, LayerMapSource, ProjectLayer};
pub use mock::MockRuleEvaluator;
pub use pipeline::{FindingBatchCallback, ScanOutcome, ScanPipeline};
pub use redaction::{redact_chunk, redact_source, REDACTED_SECRET};
pub use report::{
    FindingReport, ScanReport, ScanStatsReport, SessionReport, SeverityTotals, ToolInfo,
};
pub use router::RuleEngineRouter;
pub use rules_manifest::RuleManifest;
pub use scanner::{ParallelFileScanner, MAX_FILE_BYTES, MAX_LINE_LENGTH};
pub use sinks::CollectingChunkSink;
pub use throttler::{
    FindingBatch, FindingThrottler, ThrottlerHandle, ThrottlerSender, DEFAULT_FLUSH_INTERVAL,
};
pub use tier0::Tier0RuleEvaluator;
