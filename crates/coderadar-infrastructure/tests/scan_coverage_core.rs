//! File-level coverage and nullable compliance for scans.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use coderadar_domain::{
    CodeChunk, DomainError, RuleEvaluationResult, RuleSpec, ScanCoverage, ScanMode, ScanSession,
};
use coderadar_infrastructure::{GitChangesetReader, MockRuleEvaluator, ScanPipeline};
use coderadar_usecases::{BoxFuture, ChunkSink, SemanticRuleEvaluator};
use git2::Repository;
use tempfile::TempDir;

struct AcceptChunks;

impl ChunkSink for AcceptChunks {
    fn send_chunk(&self, _: CodeChunk) -> Result<(), DomainError> {
        Ok(())
    }
}

#[test]
fn scanner_does_not_claim_buffered_chunks_are_analyzed() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    let coverage = coderadar_infrastructure::ParallelFileScanner::new()
        .stream_project_with_coverage(
            dir.path().to_str().unwrap(),
            ScanMode::FullScan,
            &AcceptChunks,
        )
        .unwrap();
    assert_eq!(coverage.analyzed, 0);
    assert_eq!(coverage.incomplete, 1);
}

struct FailingEvaluator;

impl SemanticRuleEvaluator for FailingEvaluator {
    fn evaluate_batch<'a>(
        &'a self,
        _: Vec<CodeChunk>,
        _: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async { Err(DomainError::External("evaluation failed".into())) })
    }
}

#[test]
fn failure_during_final_flush_returns_error_instead_of_completed_scan() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    let error = ScanPipeline::new(Arc::new(FailingEvaluator), vec![])
        .run(dir.path())
        .unwrap_err();
    assert!(matches!(error, DomainError::External(message) if message == "evaluation failed"));
}

#[test]
fn empty_session_has_unknown_coverage_and_no_score() {
    let session = ScanSession::new("id", "root", ScanMode::FullScan, 0);
    assert_eq!(session.coverage, None);
    assert_eq!(session.compliance_percent(), None);
    assert_eq!(session.summary().coverage, None);
    assert_eq!(session.summary().compliance_percent, None);
}

struct ChangeIndexDuringEvaluation {
    root: PathBuf,
    changed: AtomicBool,
}

impl SemanticRuleEvaluator for ChangeIndexDuringEvaluation {
    fn evaluate_batch<'a>(
        &'a self,
        _: Vec<CodeChunk>,
        _: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            if !self.changed.swap(true, Ordering::SeqCst) {
                fs::write(
                    self.root.join("a.ts"),
                    "export const b = 2;\nexport const c = 3;\n",
                )
                .unwrap();
                let repo = Repository::open(&self.root).unwrap();
                let mut index = repo.index().unwrap();
                index.add_path(Path::new("a.ts")).unwrap();
                index.write().unwrap();
            }
            Ok(vec![])
        })
    }
}

#[test]
fn staged_hygiene_and_coverage_use_the_scanned_snapshot_when_index_changes() {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("a.ts"), "export const a = 1;\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.ts")).unwrap();
    index.write().unwrap();
    let evaluator = Arc::new(ChangeIndexDuringEvaluation {
        root: dir.path().into(),
        changed: AtomicBool::new(false),
    });
    let pipeline = ScanPipeline::new(evaluator.clone(), vec![])
        .with_ast_batch_size(1)
        .with_changeset_gateway(Arc::new(GitChangesetReader::new()));
    let result = pipeline.run_mode(dir.path(), ScanMode::StagedDiff).unwrap();
    assert!(evaluator.changed.load(Ordering::SeqCst));
    assert_eq!(result.pull_request.unwrap().additions, 1);
    assert_eq!(result.session.coverage.unwrap().analyzed, 1);
}

#[test]
fn scan_counts_file_outcomes_once() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("good.ts"), "export const a = 1;\n").unwrap();
    fs::write(dir.path().join("other.py"), "pass\n").unwrap();
    fs::write(dir.path().join("invalid.ts"), [0xff]).unwrap();
    fs::write(dir.path().join("long.ts"), "a".repeat(1001)).unwrap();
    fs::write(dir.path().join("big.ts"), vec![b'a'; 1024 * 1024 + 1]).unwrap();
    let outcome = ScanPipeline::new(Arc::new(MockRuleEvaluator::new()), vec![])
        .run(dir.path())
        .unwrap();
    assert_eq!(
        outcome.session.coverage,
        Some(ScanCoverage {
            discovered: 5,
            supported: 4,
            analyzed: 1,
            unsupported: 1,
            excluded: 0,
            exclusions_complete: Some(false),
            unreadable: 0,
            oversized: 2,
            invalid_encoding: 1,
            incomplete: 0,
        })
    );
    assert_eq!(outcome.session.compliance_percent(), Some(100));
}

#[test]
fn staged_scan_counts_snapshot_file_outcomes_and_exclusions() {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("good.ts"), "export const a = 1;\n").unwrap();
    fs::write(dir.path().join("other.py"), "pass\n").unwrap();
    fs::write(dir.path().join("invalid.ts"), [0xff]).unwrap();
    fs::write(dir.path().join(".hidden.ts"), "export const a = 1;\n").unwrap();
    let mut index = repo.index().unwrap();
    for name in ["good.ts", "other.py", "invalid.ts", ".hidden.ts"] {
        index.add_path(Path::new(name)).unwrap();
    }
    index.write().unwrap();
    let pipeline = ScanPipeline::new(Arc::new(MockRuleEvaluator::new()), vec![])
        .with_changeset_gateway(Arc::new(GitChangesetReader::new()));
    let result = pipeline.run_mode(dir.path(), ScanMode::StagedDiff).unwrap();
    assert_eq!(
        result.session.coverage,
        Some(ScanCoverage {
            discovered: 3,
            supported: 2,
            analyzed: 1,
            unsupported: 1,
            excluded: 1,
            exclusions_complete: Some(true),
            unreadable: 0,
            oversized: 0,
            invalid_encoding: 1,
            incomplete: 0,
        })
    );
}
