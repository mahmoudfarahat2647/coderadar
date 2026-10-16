//! CodeRadar Studio — Layer 4: Frameworks & Drivers.
//!
//! The outermost circle. Concrete drivers live here: the Tauri webview window,
//! the parallel file scanner, the Jev HTTP client, the SQLite repository and
//! the rules-manifest loader. This is the only layer allowed to depend on
//! external crates such as `serde`, `tokio`, `reqwest` or `sqlx`, and it may
//! depend inward on every other layer.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod rules_manifest;
mod sinks;

pub use rules_manifest::RuleManifest;
pub use sinks::CollectingChunkSink;
