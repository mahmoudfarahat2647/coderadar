//! Scan-history integration tests.
//!
//! These tests exercise the SQLite repository through its public API only:
//! the ports of Layer 2 are driven exactly as the Tauri layer will drive them.

use std::path::Path;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use coderadar_domain::{Finding, ScanMode, ScanSession, Severity};
use coderadar_infrastructure::{FindingSnapshot, SqliteScanHistoryRepository};
use coderadar_usecases::ScanHistoryRepository;

/// Block the current thread until `future` completes.
///
/// The infrastructure crate keeps its own executor internally; the integration
/// tests need a tiny local copy because `block_on` is not part of the public
/// API.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct ThreadWaker {
        thread: std::thread::Thread,
    }

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.thread.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.thread.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker {
        thread: std::thread::current(),
    }));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

fn session(id: &str, finished: u64, files: usize, findings: Vec<Finding>) -> ScanSession {
    let mut session = ScanSession::new(
        id,
        "/workspace",
        ScanMode::FullScan,
        finished.saturating_sub(10),
    );
    session.finished_at_unix_ms = Some(finished);
    session.files_scanned = files;
    for finding in findings {
        session.push(finding);
    }
    session
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

    let history = block_on(repository.get_history(10)).expect("history");
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

    let history = block_on(repository.get_history(1)).expect("history");
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

    let history = block_on(repository.get_history(10)).expect("history");
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
    let history = block_on(repository.get_history(10)).expect("history");
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

    let history = block_on(repository.get_history(10)).expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].files_scanned, 9);
    assert_eq!(history[0].finished_at_unix_ms, 2_000);
}
