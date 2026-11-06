//! Scan-scoped fallback provenance through the public evaluator boundary.
use coderadar_domain::{CodeChunk, DomainError, EvaluationEngine, RuleEvaluationResult, RuleSpec};
use coderadar_infrastructure::{block_on, EngineMode, EngineStatus, FallbackRuleEvaluator};
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct FirstFails(AtomicUsize);
impl SemanticRuleEvaluator for FirstFails {
    fn evaluate_batch<'a>(
        &'a self,
        _: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(DomainError::External("failed".into()))
            } else {
                Ok(rules
                    .iter()
                    .map(|rule| {
                        RuleEvaluationResult::new(
                            rule.id.clone(),
                            EvaluationEngine::Tier1Semantic,
                            vec![],
                        )
                    })
                    .collect())
            }
        })
    }
}
struct Succeeds;
impl SemanticRuleEvaluator for Succeeds {
    fn evaluate_batch<'a>(
        &'a self,
        _: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async {
            Ok(rules
                .iter()
                .map(|rule| {
                    RuleEvaluationResult::new(
                        rule.id.clone(),
                        EvaluationEngine::MockHeuristic,
                        vec![],
                    )
                })
                .collect())
        })
    }
}

#[test]
fn unused_live_scan_does_not_claim_a_fallback() {
    let (_, usage) = FallbackRuleEvaluator::new_scoped(
        Arc::new(Succeeds),
        Arc::new(Succeeds),
        Arc::new(EngineStatus::new(EngineMode::Tier1Untested)),
    );
    assert_eq!(usage.snapshot().legacy_engine_mode(), "jev_untested");
}

#[test]
fn concurrent_scans_keep_usage_separate() {
    let status = Arc::new(EngineStatus::new(EngineMode::Tier1Untested));
    let (failing, fallback_usage) = FallbackRuleEvaluator::new_scoped(
        Arc::new(FirstFails(AtomicUsize::new(0))),
        Arc::new(Succeeds),
        status.clone(),
    );
    let (healthy, live_usage) =
        FallbackRuleEvaluator::new_scoped(Arc::new(Succeeds), Arc::new(Succeeds), status);
    std::thread::scope(|scope| {
        scope.spawn(|| block_on(failing.evaluate_batch(vec![], &[])).unwrap());
        scope.spawn(|| block_on(healthy.evaluate_batch(vec![], &[])).unwrap());
    });
    assert_eq!(fallback_usage.snapshot().fallback_batches, 1);
    assert_eq!(fallback_usage.snapshot().primary_batches, 0);
    assert_eq!(live_usage.snapshot().primary_batches, 1);
    assert_eq!(live_usage.snapshot().fallback_batches, 0);
}

#[test]
fn recovery_does_not_erase_scan_fallback_and_scans_are_isolated() {
    let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
    let (scan_a, usage_a) = FallbackRuleEvaluator::new_scoped(
        Arc::new(FirstFails(AtomicUsize::new(0))),
        Arc::new(Succeeds),
        status.clone(),
    );
    let (scan_b, usage_b) =
        FallbackRuleEvaluator::new_scoped(Arc::new(Succeeds), Arc::new(Succeeds), status.clone());
    block_on(scan_a.evaluate_batch(vec![], &[])).unwrap();
    block_on(scan_b.evaluate_batch(vec![], &[])).unwrap();
    block_on(scan_a.evaluate_batch(vec![], &[])).unwrap();
    assert_eq!(status.mode(), EngineMode::Tier1Live);
    assert_eq!(usage_a.snapshot().legacy_engine_mode(), "mock_fallback");
    assert_eq!(usage_a.snapshot().primary_batches, 1);
    assert_eq!(usage_a.snapshot().fallback_batches, 1);
    let summary: coderadar_domain::ScanUsage = usage_a.snapshot().into();
    assert_eq!(summary.primary_batches, 1);
    assert_eq!(summary.fallback_batches, 1);
    assert_eq!(summary.fallback_failures, 0);
    assert_eq!(usage_b.snapshot().legacy_engine_mode(), "jev");
    assert_eq!(usage_b.snapshot().fallback_batches, 0);
}

#[test]
fn live_engine_exposes_a_separate_evaluator_per_scan() {
    let engine = coderadar_infrastructure::SemanticEngine::live(
        coderadar_infrastructure::JevConfig::default(),
    )
    .unwrap();
    let (_, first) = engine.scoped_evaluator().expect("live engine");
    let (_, second) = engine.scoped_evaluator().expect("live engine");
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(coderadar_infrastructure::SemanticEngine::mock()
        .scoped_evaluator()
        .is_none());
}

#[test]
fn full_scan_retains_fallback_label_after_recovery() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..40 {
        std::fs::write(
            dir.path().join(format!("file{index}.ts")),
            "export function f() { return 1; }\n",
        )
        .unwrap();
    }
    let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
    let (evaluator, usage) = FallbackRuleEvaluator::new_scoped(
        Arc::new(FirstFails(AtomicUsize::new(0))),
        Arc::new(Succeeds),
        status.clone(),
    );
    let report =
        coderadar_infrastructure::scan_project_with(dir.path(), Arc::new(evaluator), || {
            usage.snapshot().legacy_engine_mode().to_string()
        })
        .unwrap();
    assert_eq!(report.session.engine_mode, "mock_fallback");
    assert!(usage.snapshot().primary_batches > 0);
    assert!(usage.snapshot().fallback_batches > 0);
    assert_eq!(status.mode(), EngineMode::Tier1Live);
}

#[test]
fn public_engine_streaming_persists_the_same_usage_as_the_json_report() {
    use coderadar_usecases::ScanHistoryRepository;
    use std::io::{Read, Write};
    use std::time::Duration;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 8192];
        let _ = stream.read(&mut request);
        stream
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.ts"), "export const x = 1;\n").unwrap();
    let engine =
        coderadar_infrastructure::SemanticEngine::live(coderadar_infrastructure::JevConfig {
            endpoint,
            max_retries: 0,
            ..Default::default()
        })
        .unwrap();
    let repo = coderadar_infrastructure::SqliteScanHistoryRepository::open_in_memory().unwrap();
    let report = coderadar_infrastructure::scan_project_streaming_with_engine_history_mode(
        dir.path(),
        &engine,
        Duration::from_millis(1),
        coderadar_domain::ScanMode::FullScan,
        |_| {},
        &repo,
    )
    .unwrap();
    server.join().unwrap();
    let json = serde_json::to_value(report).unwrap();
    let rows = block_on(repo.get_history(10, None)).unwrap();
    let usage = rows[0].usage.unwrap();
    assert!(usage.fallback_batches > 0);
    assert_eq!(json["session"]["engine_mode"], "mock_fallback");
    assert_eq!(
        json["session"]["usage"]["primary_batches"],
        usage.primary_batches
    );
    assert_eq!(
        json["session"]["usage"]["fallback_batches"],
        usage.fallback_batches
    );
    assert_eq!(
        json["session"]["usage"]["fallback_failures"],
        usage.fallback_failures
    );
}
