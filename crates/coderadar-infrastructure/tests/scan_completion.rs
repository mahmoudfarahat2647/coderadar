use std::sync::Arc;

use coderadar_domain::{CodeChunk, DomainError, RuleEvaluationResult, RuleSpec};
use coderadar_infrastructure::{
    LayerMapSource, RuleManifest, ScanPipeline, ScanReport, SqliteScanHistoryRepository,
};
use coderadar_usecases::{BoxFuture, ScanHistoryRepository, SemanticRuleEvaluator};

mod common;

struct EmptyEvaluator;

impl SemanticRuleEvaluator for EmptyEvaluator {
    fn evaluate_batch<'a>(
        &'a self,
        _: Vec<CodeChunk>,
        _: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async { Ok(vec![]) })
    }
}

#[test]
fn omitted_rule_result_cannot_be_reported_complete() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    let manifest = RuleManifest::embedded().unwrap();
    let outcome =
        ScanPipeline::new(Arc::new(EmptyEvaluator), manifest.rules.clone()).run(dir.path());
    assert!(
        outcome.is_err() || outcome.as_ref().unwrap().session.completion != Some("complete".into())
    );
}

#[test]
fn completed_empty_scan_round_trips_completion_through_history() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = ScanPipeline::new(Arc::new(EmptyEvaluator), vec![])
        .run(dir.path())
        .unwrap();
    let report = ScanReport::from_scan(
        &outcome,
        &RuleManifest::embedded().unwrap(),
        LayerMapSource::Fallback,
        0,
    );
    assert_eq!(report.scan.completion, Some("complete"));
    let repository = SqliteScanHistoryRepository::open_in_memory().unwrap();
    common::block_on(repository.save_scan_session(&outcome.session)).unwrap();
    let rows = common::block_on(repository.get_history(10, None)).unwrap();
    assert_eq!(rows[0].completion.as_deref(), Some("complete"));
}

#[test]
fn real_scan_and_skipped_supported_file_have_distinct_statuses() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    let report = coderadar_infrastructure::scan_project(dir.path()).unwrap();
    assert_eq!(report.scan.completion, Some("complete"));
    std::fs::write(dir.path().join("bad.ts"), [0xff]).unwrap();
    let report = coderadar_infrastructure::scan_project(dir.path()).unwrap();
    assert_eq!(report.scan.completion, Some("partial"));
}
