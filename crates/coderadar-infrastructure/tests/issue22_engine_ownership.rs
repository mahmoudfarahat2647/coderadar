//! Complete-scan regression coverage for local rule ownership and Jev failures.
use std::collections::BTreeMap;
use std::sync::Arc;

use coderadar_domain::CodeChunk;
use coderadar_infrastructure::{
    block_on, scan_project, scan_project_with, EngineMode, EngineStatus, FallbackRuleEvaluator,
    JevAnswer, JevConfig, JevRequest, JevResponse, JevRuleEvaluatorAdapter, JevTransport,
    JevTransportError, MockRuleEvaluator, RuleManifest,
};
use coderadar_usecases::SemanticRuleEvaluator;

struct Answers {
    invalid: bool,
}
impl JevTransport for Answers {
    fn decide(&self, request: &JevRequest) -> Result<JevResponse, JevTransportError> {
        let answers = if self.invalid {
            BTreeMap::new()
        } else {
            request
                .questions
                .keys()
                .map(|id| (id.clone(), JevAnswer::Noul { noul: 0.0 }))
                .collect()
        };
        Ok(JevResponse { answers })
    }
}

struct Fixed(BTreeMap<String, JevAnswer>);
impl JevTransport for Fixed {
    fn decide(&self, _: &JevRequest) -> Result<JevResponse, JevTransportError> {
        Ok(JevResponse {
            answers: self.0.clone(),
        })
    }
}

#[test]
fn invalid_or_missing_provider_answers_fail_instead_of_appearing_clean() {
    let rule = RuleManifest::embedded()
        .unwrap()
        .rule_by_id("SEC-JWT-003")
        .unwrap()
        .clone();
    for answers in [
        BTreeMap::new(),
        BTreeMap::from([(rule.id.clone(), JevAnswer::Other)]),
        BTreeMap::from([(rule.id.clone(), JevAnswer::Noul { noul: f32::NAN })]),
        BTreeMap::from([(rule.id.clone(), JevAnswer::Noul { noul: 1.1 })]),
        BTreeMap::from([
            (rule.id.clone(), JevAnswer::Noul { noul: 0.0 }),
            ("unknown".into(), JevAnswer::Noul { noul: 0.0 }),
        ]),
    ] {
        let adapter = JevRuleEvaluatorAdapter::with_transport(
            JevConfig::default(),
            Arc::new(Fixed(answers)),
            Arc::new(EngineStatus::new(EngineMode::Tier1Live)),
        );
        assert!(block_on(adapter.evaluate_batch(
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            std::slice::from_ref(&rule)
        ))
        .is_err());
    }
}

#[test]
fn duplicate_answer_keys_are_rejected() {
    let json = r#"{"answers":{"SEC-JWT-003":{"type":"noul","noul":0.1},"SEC-JWT-003":{"type":"noul","noul":0.9}}}"#;
    assert!(serde_json::from_str::<JevResponse>(json).is_err());
}

#[test]
fn oversized_http_response_fails_the_scan() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 8192];
        let _ = stream.read(&mut request).unwrap();
        let body = vec![b' '; 256 * 1024 + 1];
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        let _ = stream.write_all(&body);
    });
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.ts"), "export const x = 1;\n").unwrap();
    let adapter = JevRuleEvaluatorAdapter::new(
        JevConfig {
            endpoint,
            max_retries: 0,
            ..JevConfig::default()
        },
        Arc::new(EngineStatus::new(EngineMode::Tier1Live)),
    )
    .unwrap();
    let result = scan_project_with(root.path(), Arc::new(adapter), || "jev".into());
    server.join().unwrap();
    assert!(
        result.is_err(),
        "oversized responses must not yield clean scans"
    );
}

#[test]
fn staged_live_scan_runs_both_local_module_rules() {
    let root = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(root.path()).unwrap();
    std::fs::write(
        root.path().join("userController.ts"),
        "res.json(userEntity);\nconst fd = fs.open(path, 'r');\n",
    )
    .unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_path(std::path::Path::new("userController.ts"))
        .unwrap();
    index.write().unwrap();
    let adapter = JevRuleEvaluatorAdapter::with_transport(
        JevConfig::default(),
        Arc::new(Answers { invalid: false }),
        Arc::new(EngineStatus::new(EngineMode::Tier1Live)),
    );
    let report = coderadar_infrastructure::scan_project_with_mode(
        root.path(),
        Arc::new(adapter),
        coderadar_domain::ScanMode::StagedDiff,
        || "jev".into(),
    )
    .unwrap();
    for id in ["ARCH-BND-004", "PERF-HDL-002"] {
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == id && f.engine == "mock"),
            "missing {id}"
        );
    }
}

#[test]
fn module_heuristics_run_in_offline_live_and_fallback_scans() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("userController.ts"),
        "res.json(userEntity);\nconst fd = fs.open(path, 'r');\n",
    )
    .unwrap();
    let offline = scan_project(root.path()).unwrap();
    let expected: Vec<_> = offline
        .findings
        .iter()
        .filter(|f| ["ARCH-BND-004", "PERF-HDL-002"].contains(&f.rule_id.as_str()))
        .map(|f| (f.rule_id.clone(), f.line_start, f.engine.clone()))
        .collect();
    assert_eq!(expected.len(), 2);
    for invalid in [false, true] {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
        let jev = Arc::new(JevRuleEvaluatorAdapter::with_transport(
            JevConfig::default(),
            Arc::new(Answers { invalid }),
            status.clone(),
        ));
        let evaluator = Arc::new(FallbackRuleEvaluator::new(
            jev,
            Arc::new(MockRuleEvaluator::new()),
            status.clone(),
        ));
        let report = scan_project_with(root.path(), evaluator, || "jev".to_string()).unwrap();
        let actual: Vec<_> = report
            .findings
            .iter()
            .filter(|f| ["ARCH-BND-004", "PERF-HDL-002"].contains(&f.rule_id.as_str()))
            .map(|f| (f.rule_id.clone(), f.line_start, f.engine.clone()))
            .collect();
        assert_eq!(actual, expected);
        if invalid {
            assert!(status.snapshot().fallback_events > 0);
        }
    }
}

#[test]
fn router_rejects_unknown_and_misconfigured_rule_ownership() {
    use coderadar_domain::{EvaluationEngine, RuleScope};
    use coderadar_infrastructure::{LayerMap, RuleEngineRouter};

    let manifest = RuleManifest::embedded().unwrap();
    RuleEngineRouter::validate_ownership(&manifest.rules).unwrap();
    let router = RuleEngineRouter::new(Arc::new(LayerMap::fallback()));
    let mut unknown = manifest.rule_by_id("SEC-JWT-003").unwrap().clone();
    unknown.id = "UNKNOWN-001".into();
    let mut wrong_scope = manifest.rule_by_id("ARCH-BND-004").unwrap().clone();
    wrong_scope.scope = RuleScope::AstChunk;
    let mut wrong_engine = manifest.rule_by_id("SEC-JWT-003").unwrap().clone();
    wrong_engine.engine = EvaluationEngine::Tier0Static;
    for rule in [unknown, wrong_scope, wrong_engine] {
        assert!(block_on(router.evaluate_batch(vec![], &[rule])).is_err());
    }
    let duplicate = manifest.rules[0].clone();
    assert!(RuleEngineRouter::validate_ownership(&[duplicate.clone(), duplicate]).is_err());
}

#[test]
fn staged_compositions_preserve_local_findings_and_clean_cases() {
    use coderadar_domain::ScanMode;
    use coderadar_infrastructure::scan_project_with_mode;

    let root = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(root.path()).unwrap();
    std::fs::write(
        root.path().join("userController.ts"),
        "res.json(userEntity);\nconst fd = fs.open(path, 'r');\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("safeController.ts"),
        "res.json(userDto);\nconst fd = fs.open(path, 'r');\nfs.close(fd);\n",
    )
    .unwrap();
    let mut index = repo.index().unwrap();
    for path in ["userController.ts", "safeController.ts"] {
        index.add_path(std::path::Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let mut expected = None;
    for mode in 0..3 {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
        let jev = Arc::new(JevRuleEvaluatorAdapter::with_transport(
            JevConfig::default(),
            Arc::new(Answers { invalid: mode == 2 }),
            status.clone(),
        ));
        let evaluator: Arc<dyn SemanticRuleEvaluator> = if mode == 0 {
            Arc::new(MockRuleEvaluator::new())
        } else {
            Arc::new(FallbackRuleEvaluator::new(
                jev,
                Arc::new(MockRuleEvaluator::new()),
                status.clone(),
            ))
        };
        let report = scan_project_with_mode(root.path(), evaluator, ScanMode::StagedDiff, || {
            "mock".into()
        })
        .unwrap();
        let mut actual: Vec<_> = report
            .findings
            .iter()
            .filter(|f| ["ARCH-BND-004", "PERF-HDL-002"].contains(&f.rule_id.as_str()))
            .map(|f| {
                (
                    f.rule_id.clone(),
                    f.file.clone(),
                    f.line_start,
                    f.engine.clone(),
                )
            })
            .collect();
        actual.sort();
        assert_eq!(actual.len(), 2);
        assert!(actual
            .iter()
            .all(|(_, path, _, engine)| path.ends_with("userController.ts") && engine == "mock"));
        if let Some(expected) = &expected {
            assert_eq!(&actual, expected);
        } else {
            expected = Some(actual);
        }
        if mode == 2 {
            assert!(status.snapshot().fallback_events > 0);
        }
    }
}
