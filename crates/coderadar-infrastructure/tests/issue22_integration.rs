use coderadar_domain::{CodeChunk, DomainError, RuleEvaluationResult, RuleSpec};
use coderadar_infrastructure::{
    block_on, scan_project_with_engine, EngineMode, EngineStatus, FallbackRuleEvaluator,
    InMemoryCredentialStore, JevConfig, JevDiagnostics, MockRuleEvaluator, ProviderState,
    SemanticEngine,
};
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};
use std::sync::Arc;

struct SecretError;
impl SemanticRuleEvaluator for SecretError {
    fn evaluate_batch<'a>(
        &'a self,
        _: Vec<CodeChunk>,
        _: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async {
            Err(DomainError::External(
                "secret-key and source contents".into(),
            ))
        })
    }
}

#[test]
fn fallback_status_does_not_disclose_provider_errors() {
    let status = Arc::new(EngineStatus::new(EngineMode::Tier1Untested));
    let evaluator = FallbackRuleEvaluator::new(
        Arc::new(SecretError),
        Arc::new(MockRuleEvaluator::new()),
        status.clone(),
    );
    block_on(evaluator.evaluate_batch(vec![], &[])).unwrap();
    let output = serde_json::to_string(&status.snapshot()).unwrap();
    assert!(!output.contains("secret-key"));
    assert!(!output.contains("source contents"));
    assert!(status.snapshot().degraded);
}

#[test]
fn configured_engine_is_not_live_until_a_successful_evaluation() {
    let engine = SemanticEngine::live(JevConfig::default()).unwrap();
    let status = engine.status().snapshot();
    assert!(!status.is_live);
    assert_eq!(status.badge, "Jev Tier 1 (untested)");
}

#[test]
fn diagnostics_resolve_store_credentials_without_exposing_them() {
    let store = InMemoryCredentialStore::with_key("private-test-key");
    let diagnostics = JevDiagnostics::from_store(&store);
    let result = diagnostics.initial_state();
    assert_eq!(result.state, ProviderState::ConfiguredUntested);
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("private-test-key"));
}

#[test]
fn engine_composition_runs_an_offline_scan() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("example.ts"), "export const x = 1;\n").unwrap();
    let report = scan_project_with_engine(dir.path(), &SemanticEngine::mock()).unwrap();
    assert_eq!(report.session.engine_mode, "mock");
}
