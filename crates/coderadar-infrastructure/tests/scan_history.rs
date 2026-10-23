//! Scan-history integration tests.
//!
//! These tests exercise the SQLite repository through its public API only:
//! the ports of Layer 2 are driven exactly as the Tauri layer will drive them.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use coderadar_domain::{Finding, ScanMode, ScanSession, Severity};
use coderadar_infrastructure::{
    FindingBatch, FindingSnapshot, MockRuleEvaluator, ScanReport, SqliteScanHistoryRepository,
};
use coderadar_usecases::ScanHistoryRepository;

use common::{block_on, FailingHistory};

/// The repository used by the end-to-end history tests.
const FIXTURE_REPOSITORY: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/known-bad-repo");

fn session(id: &str, finished: u64, files: usize, findings: Vec<Finding>) -> ScanSession {
    session_of(id, "/workspace", finished, files, findings)
}

/// A session of the given workspace.
fn session_of(
    id: &str,
    root: &str,
    finished: u64,
    files: usize,
    findings: Vec<Finding>,
) -> ScanSession {
    let mut session = ScanSession::new(id, root, ScanMode::FullScan, finished.saturating_sub(10));
    session.finished_at_unix_ms = Some(finished);
    session.files_scanned = files;
    for finding in findings {
        session.push(finding);
    }
    session
}

#[test]
fn the_history_can_be_scoped_to_one_workspace() {
    let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");

    let first = session_of("alpha-1", "/workspace/alpha", 1_000, 4, Vec::new());
    let second = session_of("beta-1", "/workspace/beta", 2_000, 8, Vec::new());
    let third = session_of("alpha-2", "/workspace/alpha", 3_000, 6, Vec::new());
    block_on(repository.save_scan_session(&first)).expect("save");
    block_on(repository.save_scan_session(&second)).expect("save");
    block_on(repository.save_scan_session(&third)).expect("save");

    let alpha = block_on(repository.get_history(10, Some("/workspace/alpha"))).expect("history");
    assert_eq!(
        alpha.len(),
        2,
        "only the sessions of the workspace are returned"
    );
    assert_eq!(alpha[0].id, "alpha-2");
    assert_eq!(alpha[1].id, "alpha-1");

    let beta = block_on(repository.get_history(10, Some("/workspace/beta"))).expect("history");
    assert_eq!(beta.len(), 1);
    assert_eq!(beta[0].id, "beta-1");

    let unknown = block_on(repository.get_history(10, Some("/workspace/gamma"))).expect("history");
    assert!(unknown.is_empty(), "an unknown workspace has no history");

    let everything = block_on(repository.get_history(10, None)).expect("history");
    assert_eq!(
        everything.len(),
        3,
        "without a filter every session is returned"
    );
}

#[test]
fn two_sessions_are_read_back_newest_first() {
    let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");

    let older = session(
        "older",
        1_000,
        4,
        vec![Finding::new("SEC-001", "a.ts", 1, 1).with_severity(Severity::High)],
    );
    let newer = session(
        "newer",
        2_000,
        10,
        vec![
            Finding::new("SEC-002", "b.ts", 1, 1).with_severity(Severity::Critical),
            Finding::new("SEC-003", "c.ts", 1, 1).with_severity(Severity::Low),
        ],
    );

    block_on(repository.save_scan_session(&older)).expect("save");
    block_on(repository.save_scan_session(&newer)).expect("save");

    let history = block_on(repository.get_history(10, None)).expect("history");
    assert_eq!(history.len(), 2);

    assert_eq!(history[0].id, "newer");
    assert_eq!(history[0].finished_at_unix_ms, 2_000);
    assert_eq!(history[0].files_scanned, 10);
    assert_eq!(history[0].total_findings, 2);
    assert_eq!(history[0].critical, 1);
    assert_eq!(history[0].low, 1);
    assert_eq!(history[0].high, 0);
    assert_eq!(history[0].medium, 0);
    // Two of the ten analysed files carry a finding.
    assert_eq!(history[0].compliance_percent, 80);

    assert_eq!(history[1].id, "older");
    assert_eq!(history[1].files_scanned, 4);
    assert_eq!(history[1].total_findings, 1);
    assert_eq!(history[1].high, 1);
    assert_eq!(history[1].compliance_percent, 75);
}

#[test]
fn the_history_is_limited_to_the_newest_sessions() {
    let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
    for index in 0..5u64 {
        let session = session(&format!("session-{index}"), 1_000 + index, 1, Vec::new());
        block_on(repository.save_scan_session(&session)).expect("save");
    }

    let history = block_on(repository.get_history(1, None)).expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, "session-4");
    assert_eq!(history[0].finished_at_unix_ms, 1_004);
}

#[test]
fn the_findings_snapshot_round_trips() {
    let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
    let finding = Finding::new("SEC-001", "src/app.ts", 12, 14)
        .with_cwe("CWE-798")
        .with_title("Hardcoded secret")
        .with_description("An API token is embedded in the source.");
    let session = session("with-findings", 5_000, 3, vec![finding]);

    block_on(repository.save_scan_session(&session)).expect("save");

    let snapshots = repository
        .load_snapshots("with-findings")
        .expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    let snapshot: &FindingSnapshot = &snapshots[0];
    assert_eq!(snapshot.rule_id, "SEC-001");
    assert_eq!(snapshot.file_path, "src/app.ts");
    assert_eq!(snapshot.line_start, 12);
    assert_eq!(snapshot.line_end, 14);
    assert_eq!(snapshot.category, "security");
    assert_eq!(snapshot.severity, "CLEAN");
    assert_eq!(snapshot.cwe.as_deref(), Some("CWE-798"));
    assert_eq!(snapshot.title, "Hardcoded secret");
    assert_eq!(
        snapshot.description,
        "An API token is embedded in the source."
    );
}

#[test]
fn an_in_memory_repository_never_touches_the_filesystem() {
    let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
    assert_eq!(repository.path(), Path::new(":memory:"));

    let session = session("in-memory", 1_000, 2, Vec::new());
    block_on(repository.save_scan_session(&session)).expect("save");

    let history = block_on(repository.get_history(10, None)).expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, "in-memory");
}

#[test]
fn a_file_backed_repository_persists_across_reopenings() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("nested").join("history.sqlite3");

    {
        let repository = SqliteScanHistoryRepository::open(&path).expect("open");
        assert_eq!(repository.path(), path);
        let session = session("persisted", 1_000, 5, Vec::new());
        block_on(repository.save_scan_session(&session)).expect("save");
    }

    assert!(path.exists(), "the database file must have been created");

    let repository = SqliteScanHistoryRepository::open(&path).expect("open");
    let history = block_on(repository.get_history(10, None)).expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, "persisted");
    assert_eq!(history[0].files_scanned, 5);
}

#[test]
fn saving_the_same_session_twice_replaces_the_row() {
    let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
    let first = session("same-id", 1_000, 1, Vec::new());
    let second = session("same-id", 2_000, 9, Vec::new());

    block_on(repository.save_scan_session(&first)).expect("save");
    block_on(repository.save_scan_session(&second)).expect("save");

    let history = block_on(repository.get_history(10, None)).expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].files_scanned, 9);
    assert_eq!(history[0].finished_at_unix_ms, 2_000);
}

#[test]
fn a_failing_history_does_not_fail_the_scan() {
    let history = FailingHistory;

    let report: ScanReport = coderadar_infrastructure::scan_project_streaming_with_history(
        Path::new(FIXTURE_REPOSITORY),
        Arc::new(MockRuleEvaluator::new()),
        Duration::from_millis(50),
        |_batch: FindingBatch| {},
        &history,
        || "mock".to_string(),
    )
    .expect("a broken history must never invalidate a finished scan");

    assert!(
        report.scan.files_scanned > 0,
        "the scan itself must have completed"
    );
}

#[test]
fn a_completed_scan_is_persisted_with_its_compliance_score() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("history.sqlite3");
    let repository = SqliteScanHistoryRepository::open(&path).expect("open");

    let report: ScanReport = coderadar_infrastructure::scan_project_streaming_with_history(
        Path::new(FIXTURE_REPOSITORY),
        Arc::new(MockRuleEvaluator::new()),
        Duration::from_millis(50),
        |_batch: FindingBatch| {},
        &repository,
        || "mock".to_string(),
    )
    .expect("the scan succeeds");

    let history = block_on(repository.get_history(10, None)).expect("history");
    assert_eq!(history.len(), 1, "exactly one session is persisted");

    let session = &history[0];
    assert_eq!(session.id, report.session.id, "the persisted session");
    assert!(session.files_scanned > 0, "the files were counted");
    assert_eq!(session.total_findings, report.findings.len());
    assert!(
        session.compliance_percent <= 100,
        "the compliance score stays in 0..=100"
    );
    assert_eq!(session.root_dir, canonical_fixture_root());
}

#[test]
fn the_findings_of_a_completed_scan_are_snapshotted() {
    let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");

    let report: ScanReport = coderadar_infrastructure::scan_project_streaming_with_history(
        Path::new(FIXTURE_REPOSITORY),
        Arc::new(MockRuleEvaluator::new()),
        Duration::from_millis(50),
        |_batch: FindingBatch| {},
        &repository,
        || "mock".to_string(),
    )
    .expect("the scan succeeds");

    let snapshots = repository
        .load_snapshots(&report.session.id)
        .expect("snapshots");
    assert_eq!(snapshots.len(), report.findings.len());
    assert!(!snapshots.is_empty(), "the fixture carries findings");
}

/// The root directory of the fixture, in the very shape the pipeline stores it.
fn canonical_fixture_root() -> String {
    std::fs::canonicalize(FIXTURE_REPOSITORY)
        .expect("the fixture repository exists")
        .to_string_lossy()
        .replace('\\', "/")
}
