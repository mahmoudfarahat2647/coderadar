//! Runtime engine selection, telemetry and Mock fallback (`CR-SPEC-12 §4–§5`).
//!
//! [`SemanticEngine`] is the composition-root decision: it builds either the
//! offline [`MockRuleEvaluator`] or the live Jev adapter wrapped in a
//! [`FallbackRuleEvaluator`]. The shared [`EngineStatus`] records every
//! transition so the dashboard can show the *Mock Mode (fallback)* badge the
//! moment the network drops.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use coderadar_domain::{CodeChunk, DomainError, RuleEvaluationResult, RuleSpec, ScanUsage};
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};
use serde::Serialize;

use crate::jev::{JevConfig, JevRuleEvaluatorAdapter};
use crate::keychain::{resolve_api_key, CredentialStore, KeyringCredentialStore};
use crate::mock::MockRuleEvaluator;
use crate::provider_diagnostics::JevDiagnostics;

/// OpenRouter's authenticated key metadata endpoint.
pub const DEFAULT_METADATA_ENDPOINT: &str = "https://openrouter.ai/api/v1/key";

impl JevDiagnostics {
    /// Resolve the credential using the same keychain/environment precedence as scans.
    pub fn from_store(store: &dyn CredentialStore) -> Self {
        let key = resolve_api_key(store);
        Self::new(
            DEFAULT_METADATA_ENDPOINT,
            crate::jev::DEFAULT_ENDPOINT,
            crate::jev::DEFAULT_MODEL,
            key.as_deref(),
        )
    }

    /// Resolve the credential from the operating system keychain or environment.
    pub fn from_environment() -> Self {
        Self::from_store(&KeyringCredentialStore::new())
    }
}

/// Which engine is currently answering evaluations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    /// No API key: the offline heuristic engine is the only engine.
    MockPrimary,
    /// A key is configured and the live Jev engine is answering.
    Tier1Live,
    /// Jev was configured but a call failed; Mock answered instead.
    MockFallback,
    /// A key is configured but inference has not yet succeeded.
    Tier1Untested,
}

impl EngineMode {
    fn as_u8(self) -> u8 {
        match self {
            Self::MockPrimary => 0,
            Self::Tier1Live => 1,
            Self::MockFallback => 2,
            Self::Tier1Untested => 3,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Tier1Live,
            2 => Self::MockFallback,
            3 => Self::Tier1Untested,
            _ => Self::MockPrimary,
        }
    }

    /// Stable token exposed to the UI/webview.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MockPrimary => "mock",
            Self::Tier1Live => "jev",
            Self::MockFallback => "mock_fallback",
            Self::Tier1Untested => "jev_untested",
        }
    }
}

/// Shared, thread-safe engine telemetry.
#[derive(Debug)]
pub struct EngineStatus {
    mode: AtomicU8,
    reason: Mutex<Option<String>>,
    tier1_requests: AtomicU64,
    fallback_events: AtomicU64,
    last_latency_ms: AtomicU64,
    max_latency_ms: AtomicU64,
    total_latency_ms: AtomicU64,
}

impl EngineStatus {
    /// Create a status in `mode` with zeroed counters.
    pub fn new(mode: EngineMode) -> Self {
        Self {
            mode: AtomicU8::new(mode.as_u8()),
            reason: Mutex::new(None),
            tier1_requests: AtomicU64::new(0),
            fallback_events: AtomicU64::new(0),
            last_latency_ms: AtomicU64::new(0),
            max_latency_ms: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
        }
    }

    /// Current engine mode.
    pub fn mode(&self) -> EngineMode {
        EngineMode::from_u8(self.mode.load(Ordering::Relaxed))
    }

    /// Record a successful Jev round trip and its latency.
    pub fn record_tier1_request(&self, latency_ms: u64) {
        self.tier1_requests.fetch_add(1, Ordering::Relaxed);
        self.last_latency_ms.store(latency_ms, Ordering::Relaxed);
        self.max_latency_ms.fetch_max(latency_ms, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    /// Record that Jev failed and Mock took over for this batch.
    pub fn record_fallback(&self, reason: impl Into<String>) {
        self.fallback_events.fetch_add(1, Ordering::Relaxed);
        self.mode
            .store(EngineMode::MockFallback.as_u8(), Ordering::Relaxed);
        if let Ok(mut guard) = self.reason.lock() {
            *guard = Some(reason.into());
        }
    }

    /// Record that Jev answered again after a fallback.
    pub fn record_recovery(&self) {
        if matches!(
            self.mode(),
            EngineMode::MockFallback | EngineMode::Tier1Untested
        ) {
            self.mode
                .store(EngineMode::Tier1Live.as_u8(), Ordering::Relaxed);
            if let Ok(mut guard) = self.reason.lock() {
                *guard = None;
            }
        }
    }

    /// Take an immutable snapshot for the UI layer.
    pub fn snapshot(&self) -> EngineStatusSnapshot {
        let mode = self.mode();
        let reason = self.reason.lock().ok().and_then(|guard| guard.clone());
        EngineStatusSnapshot {
            mode: mode.as_str().to_string(),
            is_live: mode == EngineMode::Tier1Live,
            degraded: mode == EngineMode::MockFallback,
            badge: match mode {
                EngineMode::Tier1Live => "Jev Tier 1 (live)",
                EngineMode::MockFallback => "Mock Mode (fallback)",
                EngineMode::MockPrimary => "Mock Mode",
                EngineMode::Tier1Untested => "Jev Tier 1 (untested)",
            }
            .to_string(),
            reason,
            tier1_requests: self.tier1_requests.load(Ordering::Relaxed),
            fallback_events: self.fallback_events.load(Ordering::Relaxed),
            last_latency_ms: self.last_latency_ms.load(Ordering::Relaxed),
            max_latency_ms: self.max_latency_ms.load(Ordering::Relaxed),
            total_latency_ms: self.total_latency_ms.load(Ordering::Relaxed),
        }
    }
}

/// Serializable engine telemetry consumed by the Tauri command layer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EngineStatusSnapshot {
    /// `mock` | `jev` | `jev_untested` | `mock_fallback`.
    pub mode: String,
    /// True when live Jev is answering.
    pub is_live: bool,
    /// True when Jev failed and Mock has taken over.
    pub degraded: bool,
    /// Human-readable status-bar badge text.
    pub badge: String,
    /// Why the engine degraded, when it did.
    pub reason: Option<String>,
    /// Successful Jev requests recorded.
    pub tier1_requests: u64,
    /// Times Jev handed over to Mock.
    pub fallback_events: u64,
    /// Latency of the most recent Jev request, in milliseconds.
    pub last_latency_ms: u64,
    /// Worst Jev latency observed, in milliseconds.
    pub max_latency_ms: u64,
    /// Sum of all recorded Jev request latencies, in milliseconds
    /// (`total / tier1_requests` is the mean request latency).
    pub total_latency_ms: u64,
}

/// Wraps a primary evaluator and transparently degrades to a fallback.
///
/// Any [`Err`] from the primary (timeout, offline, HTTP, parse) records a
/// fallback transition and re-runs the **same** batch through the fallback
/// engine, so the scan completes and the UI badge flips to Mock.
pub struct FallbackRuleEvaluator {
    primary: Arc<dyn SemanticRuleEvaluator>,
    fallback: Arc<dyn SemanticRuleEvaluator>,
    status: Arc<EngineStatus>,
    usage: Option<Arc<ScanEngineUsage>>,
}

/// Cumulative semantic evaluator usage, owned by one scan.
#[derive(Debug, Default)]
pub struct ScanEngineUsage {
    primary_batches: AtomicU64,
    fallback_batches: AtomicU64,
    fallback_failures: AtomicU64,
}

/// A snapshot of cumulative scan usage; batches may contain several chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScanEngineUsageSnapshot {
    /// Batches successfully answered by the primary evaluator.
    pub primary_batches: u64,
    /// Batches successfully answered by the fallback evaluator.
    pub fallback_batches: u64,
    /// Batches for which both evaluators failed.
    pub fallback_failures: u64,
}

impl ScanEngineUsage {
    /// Return cumulative usage for this scan.
    pub fn snapshot(&self) -> ScanEngineUsageSnapshot {
        ScanEngineUsageSnapshot {
            primary_batches: self.primary_batches.load(Ordering::Relaxed),
            fallback_batches: self.fallback_batches.load(Ordering::Relaxed),
            fallback_failures: self.fallback_failures.load(Ordering::Relaxed),
        }
    }
}

impl ScanEngineUsageSnapshot {
    /// Report label derived from this scan, including an untested live engine.
    pub fn legacy_engine_mode(&self) -> &'static str {
        if self.fallback_batches > 0 || self.fallback_failures > 0 {
            "mock_fallback"
        } else if self.primary_batches > 0 {
            "jev"
        } else {
            "jev_untested"
        }
    }
}

impl From<ScanEngineUsageSnapshot> for ScanUsage {
    fn from(value: ScanEngineUsageSnapshot) -> Self {
        Self {
            primary_batches: value.primary_batches,
            fallback_batches: value.fallback_batches,
            fallback_failures: value.fallback_failures,
        }
    }
}

impl FallbackRuleEvaluator {
    /// Build the decorator over `primary`, degrading to `fallback`.
    pub fn new(
        primary: Arc<dyn SemanticRuleEvaluator>,
        fallback: Arc<dyn SemanticRuleEvaluator>,
        status: Arc<EngineStatus>,
    ) -> Self {
        Self {
            primary,
            fallback,
            status,
            usage: None,
        }
    }

    /// Construct an evaluator and an independent usage handle for one scan.
    pub fn new_scoped(
        primary: Arc<dyn SemanticRuleEvaluator>,
        fallback: Arc<dyn SemanticRuleEvaluator>,
        status: Arc<EngineStatus>,
    ) -> (Self, Arc<ScanEngineUsage>) {
        let usage = Arc::new(ScanEngineUsage::default());
        let mut evaluator = Self::new(primary, fallback, status);
        evaluator.usage = Some(Arc::clone(&usage));
        (evaluator, usage)
    }
}

impl SemanticRuleEvaluator for FallbackRuleEvaluator {
    fn evaluate_batch<'a>(
        &'a self,
        chunks: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            match self.primary.evaluate_batch(chunks.clone(), rules).await {
                Ok(results) => {
                    if let Some(usage) = &self.usage {
                        usage.primary_batches.fetch_add(1, Ordering::Relaxed);
                    }
                    self.status.record_recovery();
                    Ok(results)
                }
                Err(_) => {
                    self.status.record_fallback("Jev request failed");
                    let result = self.fallback.evaluate_batch(chunks, rules).await;
                    if let Some(usage) = &self.usage {
                        let counter = if result.is_ok() {
                            &usage.fallback_batches
                        } else {
                            &usage.fallback_failures
                        };
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                    result
                }
            }
        })
    }
}

/// The semantic engine selected by the composition root.
pub struct SemanticEngine {
    evaluator: Arc<dyn SemanticRuleEvaluator>,
    status: Arc<EngineStatus>,
    primary: Option<Arc<dyn SemanticRuleEvaluator>>,
}

impl SemanticEngine {
    /// The offline Mock engine (no key configured).
    pub fn mock() -> Self {
        Self {
            evaluator: Arc::new(MockRuleEvaluator::new()),
            status: Arc::new(EngineStatus::new(EngineMode::MockPrimary)),
            primary: None,
        }
    }

    /// The live Jev engine wrapped in a Mock fallback.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the HTTP client cannot be built
    /// (for example an invalid timeout configuration).
    pub fn live(config: JevConfig) -> Result<Self, DomainError> {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Untested));
        let primary: Arc<dyn SemanticRuleEvaluator> =
            Arc::new(JevRuleEvaluatorAdapter::new(config, Arc::clone(&status))?);
        let fallback: Arc<dyn SemanticRuleEvaluator> = Arc::new(MockRuleEvaluator::new());
        Ok(Self {
            evaluator: Arc::new(FallbackRuleEvaluator::new(
                Arc::clone(&primary),
                fallback,
                Arc::clone(&status),
            )),
            status,
            primary: Some(primary),
        })
    }

    /// Build from a credential store: live Jev when a key resolves, else Mock.
    ///
    /// # Errors
    /// Propagates the [`SemanticEngine::live`] construction error when a key is
    /// present but the HTTP client cannot be built.
    pub fn from_store(store: &dyn CredentialStore, config: JevConfig) -> Result<Self, DomainError> {
        match resolve_api_key(store) {
            Some(api_key) => Self::live(JevConfig { api_key, ..config }),
            None => Ok(Self::mock()),
        }
    }

    /// Build from the OS keychain / `JEV_API_KEY`, degrading to Mock on any
    /// resolution or construction failure.
    pub fn from_environment() -> Self {
        Self::from_store(&KeyringCredentialStore::new(), JevConfig::default())
            .unwrap_or_else(|_| Self::mock())
    }

    /// The evaluator to hand to the scan pipeline / router.
    pub fn evaluator(&self) -> Arc<dyn SemanticRuleEvaluator> {
        Arc::clone(&self.evaluator)
    }

    /// Create a fresh, isolated live/fallback evaluator and usage for one scan.
    /// Returns `None` for the offline engine; use [`Self::evaluator`] there.
    pub fn scoped_evaluator(
        &self,
    ) -> Option<(Arc<dyn SemanticRuleEvaluator>, Arc<ScanEngineUsage>)> {
        let primary = Arc::clone(self.primary.as_ref()?);
        let (evaluator, usage) = FallbackRuleEvaluator::new_scoped(
            primary,
            Arc::new(MockRuleEvaluator::new()),
            Arc::clone(&self.status),
        );
        Some((Arc::new(evaluator), usage))
    }

    /// The shared telemetry handle.
    pub fn status(&self) -> Arc<EngineStatus> {
        Arc::clone(&self.status)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use coderadar_domain::{
        CodeChunk, DomainError, EvaluationEngine, RuleEvaluationResult, RuleSpec,
    };
    use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};

    use super::{EngineMode, EngineStatus, EngineStatusSnapshot, FallbackRuleEvaluator};
    use crate::engine_rules::TIER0_OWNED_IDS;
    use crate::rules_manifest::RuleManifest;

    /// Evaluator that counts calls and fails until told to succeed.
    struct FlakyEvaluator {
        calls: AtomicUsize,
        fail: bool,
    }

    impl FlakyEvaluator {
        fn new(fail: bool) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail,
            }
        }
    }

    impl SemanticRuleEvaluator for FlakyEvaluator {
        fn evaluate_batch<'a>(
            &'a self,
            _chunks: Vec<CodeChunk>,
            rules: &'a [RuleSpec],
        ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::Relaxed);
                if self.fail {
                    return Err(DomainError::External("offline".to_string()));
                }
                Ok(rules
                    .iter()
                    .map(|rule| {
                        RuleEvaluationResult::new(
                            rule.id.clone(),
                            EvaluationEngine::MockHeuristic,
                            Vec::new(),
                        )
                    })
                    .collect())
            })
        }
    }

    fn rule() -> RuleSpec {
        RuleManifest::embedded()
            .expect("manifest parses")
            .rule_by_id("SEC-JWT-003")
            .expect("rule exists")
            .clone()
    }

    fn evaluate(evaluator: &FallbackRuleEvaluator) -> Vec<RuleEvaluationResult> {
        crate::executor::block_on(evaluator.evaluate_batch(
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &[rule()],
        ))
        .expect("fallback evaluation cannot fail")
    }

    #[test]
    fn primary_success_keeps_the_live_mode() {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
        let primary = Arc::new(FlakyEvaluator::new(false));
        let fallback = Arc::new(FlakyEvaluator::new(false));
        let evaluator =
            FallbackRuleEvaluator::new(primary.clone(), fallback.clone(), Arc::clone(&status));

        evaluate(&evaluator);

        assert_eq!(primary.calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback.calls.load(Ordering::Relaxed), 0);
        assert_eq!(status.mode(), EngineMode::Tier1Live);
        assert!(!status.snapshot().degraded);
    }

    #[test]
    fn primary_failure_degrades_to_the_fallback_and_records_it() {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
        let primary = Arc::new(FlakyEvaluator::new(true));
        let fallback = Arc::new(FlakyEvaluator::new(false));
        let evaluator =
            FallbackRuleEvaluator::new(primary.clone(), fallback.clone(), Arc::clone(&status));

        let results = evaluate(&evaluator);

        assert_eq!(primary.calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback.calls.load(Ordering::Relaxed), 1);
        assert_eq!(results.len(), 1);
        assert_eq!(status.mode(), EngineMode::MockFallback);

        let snapshot = status.snapshot();
        assert!(snapshot.degraded);
        assert_eq!(snapshot.badge, "Mock Mode (fallback)");
        assert_eq!(snapshot.fallback_events, 1);
        assert_eq!(snapshot.reason.as_deref(), Some("Jev request failed"));
    }

    #[test]
    fn recovery_flips_back_to_live_after_a_successful_call() {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
        let primary = Arc::new(FlakyEvaluator::new(true));
        let fallback = Arc::new(FlakyEvaluator::new(false));
        let evaluator =
            FallbackRuleEvaluator::new(primary.clone(), fallback.clone(), Arc::clone(&status));

        evaluate(&evaluator);
        assert_eq!(status.mode(), EngineMode::MockFallback);

        // Swap in a healthy primary by rebuilding the decorator around a new
        // successful evaluator sharing the same status handle.
        let healthy = Arc::new(FlakyEvaluator::new(false));
        let recovered =
            FallbackRuleEvaluator::new(healthy.clone(), fallback.clone(), Arc::clone(&status));
        evaluate(&recovered);

        assert_eq!(healthy.calls.load(Ordering::Relaxed), 1);
        assert_eq!(status.mode(), EngineMode::Tier1Live);
        assert_eq!(status.snapshot().reason, None);
    }

    #[test]
    fn mock_engine_reports_mock_mode() {
        let engine = super::SemanticEngine::mock();
        let snapshot: EngineStatusSnapshot = engine.status().snapshot();
        assert_eq!(snapshot.mode, "mock");
        assert!(!snapshot.is_live);
        assert!(!snapshot.degraded);
        assert_eq!(snapshot.badge, "Mock Mode");
    }

    #[test]
    fn tier0_owned_ids_are_not_semantic() {
        let manifest = RuleManifest::embedded().expect("manifest parses");
        for id in TIER0_OWNED_IDS {
            let rule = manifest.rule_by_id(id).expect("tier0 rule exists");
            assert!(!crate::engine_rules::is_semantic_rule(rule), "{id}");
        }
    }
}
