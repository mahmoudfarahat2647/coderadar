//! Coverage and scoring when findings or sources fall outside the analyzed set.
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use coderadar_domain::{
    CodeChunk, DomainError, EvaluationEngine, Finding, RuleEvaluationResult, RuleSpec,
};
use coderadar_infrastructure::ScanPipeline;
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};
use tempfile::TempDir;

struct Evaluator {
    mutate: Option<PathBuf>,
    changed: AtomicBool,
}

impl SemanticRuleEvaluator for Evaluator {
    fn evaluate_batch<'a>(
        &'a self,
        _: Vec<CodeChunk>,
        _: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            if let Some(path) = &self.mutate {
                if !self.changed.swap(true, Ordering::SeqCst) {
                    fs::write(path, "export const changed = 2;\n").unwrap();
                }
            }
            Ok(vec![RuleEvaluationResult::new(
                "test",
                EvaluationEngine::default(),
                vec![Finding::new("test", "outside.py", 1, 1)],
            )])
        })
    }
}

#[test]
fn unverified_findings_make_the_score_unavailable() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    let evaluator = Evaluator {
        mutate: None,
        changed: AtomicBool::new(false),
    };
    let outcome = ScanPipeline::new(Arc::new(evaluator), vec![])
        .run(dir.path())
        .unwrap();
    assert!(outcome.session.findings.is_empty());
    assert_eq!(outcome.session.compliance_percent(), None);
}

#[test]
fn unconsumed_source_findings_are_not_published_as_verified() {
    use coderadar_infrastructure::RuleManifest;
    use std::sync::Mutex;

    struct OutsideEvaluator;
    impl SemanticRuleEvaluator for OutsideEvaluator {
        fn evaluate_batch<'a>(
            &'a self,
            _: Vec<CodeChunk>,
            rules: &'a [RuleSpec],
        ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
            Box::pin(async move {
                Ok(rules
                    .iter()
                    .map(|rule| {
                        RuleEvaluationResult::new(
                            &rule.id,
                            EvaluationEngine::default(),
                            vec![Finding::new(&rule.id, "outside.py", 1, 1)],
                        )
                    })
                    .collect())
            })
        }
    }

    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    let rule = RuleManifest::embedded()
        .unwrap()
        .rule_by_id("SEC-TOK-002")
        .unwrap()
        .clone();
    let published = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&published);
    let outcome = ScanPipeline::new(Arc::new(OutsideEvaluator), vec![rule])
        .run_with_callback(dir.path(), move |batch| {
            captured.lock().unwrap().extend_from_slice(batch)
        })
        .unwrap();
    assert!(outcome.session.findings.is_empty());
    assert!(published.lock().unwrap().is_empty());
    assert_eq!(outcome.session.completion.as_deref(), Some("partial"));
    assert_eq!(outcome.session.coverage.unwrap().analyzed, 0);
    assert_eq!(outcome.session.coverage.unwrap().incomplete, 1);
    assert_eq!(outcome.session.compliance_percent(), None);
}

#[test]
fn changed_source_is_not_reported_as_analyzed() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.ts");
    fs::write(&path, "export const a = 1;\n").unwrap();
    let evaluator = Evaluator {
        mutate: Some(path),
        changed: AtomicBool::new(false),
    };
    let outcome = ScanPipeline::new(Arc::new(evaluator), vec![])
        .with_ast_batch_size(1)
        .run(dir.path())
        .unwrap();
    let coverage = outcome.session.coverage.unwrap();
    assert_eq!(coverage.analyzed, 0);
    assert_eq!(coverage.incomplete, 1);
    assert_eq!(outcome.session.completion.as_deref(), Some("partial"));
    assert_eq!(outcome.session.compliance_percent(), None);
}

#[test]
fn changed_source_findings_are_neither_final_nor_streamed() {
    use std::sync::Mutex;
    struct ChangingEvaluator(PathBuf);
    impl SemanticRuleEvaluator for ChangingEvaluator {
        fn evaluate_batch<'a>(
            &'a self,
            _: Vec<CodeChunk>,
            _: &'a [RuleSpec],
        ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
            Box::pin(async move {
                fs::write(&self.0, "export const changed = 2;\n").unwrap();
                Ok(vec![RuleEvaluationResult::new(
                    "test",
                    EvaluationEngine::default(),
                    vec![Finding::new("test", "a.ts", 1, 1)],
                )])
            })
        }
    }
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.ts");
    fs::write(&path, "export const a = 1;\n").unwrap();
    let streamed = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&streamed);
    let outcome = ScanPipeline::new(Arc::new(ChangingEvaluator(path)), vec![])
        .with_ast_batch_size(1)
        .run_with_callback(dir.path(), move |batch| {
            captured.lock().unwrap().extend_from_slice(batch)
        })
        .unwrap();
    assert!(outcome.session.findings.is_empty());
    assert!(streamed.lock().unwrap().is_empty());
    assert_eq!(outcome.session.coverage.unwrap().incomplete, 1);
    assert!(outcome
        .session
        .consumed_sources
        .as_ref()
        .unwrap()
        .contains_key("a.ts"));
    assert!(!outcome
        .session
        .analyzed_sources
        .as_ref()
        .unwrap()
        .contains_key("a.ts"));
}

#[test]
fn provisional_findings_precede_source_verification() {
    use std::sync::Mutex;
    struct ChangingEvaluator(PathBuf);
    impl SemanticRuleEvaluator for ChangingEvaluator {
        fn evaluate_batch<'a>(
            &'a self,
            _: Vec<CodeChunk>,
            _: &'a [RuleSpec],
        ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
            Box::pin(async move {
                fs::write(&self.0, "export const changed = 2;\n").unwrap();
                Ok(vec![RuleEvaluationResult::new(
                    "test",
                    EvaluationEngine::default(),
                    vec![Finding::new("test", "a.ts", 1, 1)],
                )])
            })
        }
    }
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.ts");
    fs::write(&path, "export const a = 1;\n").unwrap();
    let provisional = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&provisional);
    let outcome = ScanPipeline::new(Arc::new(ChangingEvaluator(path)), vec![])
        .with_ast_batch_size(1)
        .run_mode_with_provisional_callback(
            dir.path(),
            coderadar_domain::ScanMode::FullScan,
            move |batch| captured.lock().unwrap().extend_from_slice(batch),
        )
        .unwrap();
    assert!(!provisional.lock().unwrap().is_empty());
    assert!(outcome.session.findings.is_empty());
}
