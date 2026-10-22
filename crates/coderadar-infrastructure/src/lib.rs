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
mod export;
mod git_changeset;
mod hashing;
mod heuristics;
mod history;
mod jev;
mod keychain;
mod layer_map;
mod mock;
mod patching;
mod pipeline;
mod redaction;
mod report;
mod router;
mod rules_manifest;
mod rules_ota;
mod scanner;
mod sinks;
mod source_reader;
mod throttler;
mod tier0;
mod tier2;
mod workspace_guard;

pub use chunker::{detect_language, Chunker, JAVASCRIPT, TYPESCRIPT};
pub use composition::{
    scan_project, scan_project_mode, scan_project_streaming, scan_project_streaming_with,
    scan_project_streaming_with_history, scan_project_streaming_with_history_mode,
    scan_project_streaming_with_mode, scan_project_with, scan_project_with_mode,
};
pub use engine::{
    EngineMode, EngineStatus, EngineStatusSnapshot, FallbackRuleEvaluator, SemanticEngine,
};
pub use engine_rules::{is_semantic_rule, GIT_CHANGESET_IDS, TIER0_OWNED_IDS};
pub use executor::block_on;
pub use export::{
    markdown_from_report, write_markdown_report, ExportOutcome, DEFAULT_REPORT_FILE_NAME,
};
pub use git_changeset::GitChangesetReader;
pub use hashing::{sha256_hex, sha256_hex_bytes};
pub use history::{FindingSnapshot, SqliteScanHistoryRepository};
pub use jev::{
    HttpJevTransport, JevAnswer, JevConfig, JevQuestion, JevRequest, JevResponse,
    JevRuleEvaluatorAdapter, JevTransport, JevTransportError, DEFAULT_BATCH_SIZE,
    DEFAULT_CONFIDENCE_THRESHOLD, DEFAULT_ENDPOINT, DEFAULT_MAX_PARALLEL, DEFAULT_MODEL,
    DEFAULT_TIMEOUT_MS,
};
pub use keychain::{
    mask_api_key, resolve_api_key, resolve_gemini_key, CredentialStore, InMemoryCredentialStore,
    KeyringCredentialStore, GEMINI_API_KEY_ENV, GEMINI_KEYRING_ACCOUNT, JEV_API_KEY_ENV,
    KEYRING_ACCOUNT, KEYRING_SERVICE,
};
pub use layer_map::{LayerMap, LayerMapSource, ProjectLayer};
pub use mock::MockRuleEvaluator;
pub use patching::SafePatchApplier;
pub use pipeline::{FindingBatchCallback, ScanOutcome, ScanPipeline};
pub use redaction::{redact_chunk, redact_source, REDACTED_SECRET};
pub use report::{
    FindingReport, ScanReport, ScanStatsReport, SessionReport, SeverityTotals, ToolInfo,
};
pub use router::RuleEngineRouter;
pub use rules_manifest::{RuleManifest, EMBEDDED_MANIFEST_JSON};
pub use rules_ota::{
    ActiveRuleset, RulesSource, RulesStore, ACTIVE_MANIFEST_FILE, RULES_DIRECTORY,
    RULES_PUBLIC_KEY_HEX, SIGNATURE_PREFIX,
};
pub use scanner::{ParallelFileScanner, MAX_FILE_BYTES, MAX_LINE_LENGTH};
pub use sinks::CollectingChunkSink;
pub use source_reader::FileSystemSourceReader;
pub use throttler::{
    FindingBatch, FindingThrottler, ThrottlerHandle, ThrottlerSender, DEFAULT_FLUSH_INTERVAL,
};
pub use tier0::Tier0RuleEvaluator;
pub use tier2::{
    GeminiFixProvider, OllamaFixProvider, DEFAULT_GEMINI_ENDPOINT, DEFAULT_GEMINI_MODEL,
    DEFAULT_OLLAMA_ENDPOINT, DEFAULT_OLLAMA_MODEL, GEMINI_MODEL_ENV, GENERATION_TIMEOUT,
    OLLAMA_ENDPOINT_ENV, OLLAMA_MODEL_ENV,
};
pub use workspace_guard::WorkspaceGuard;
