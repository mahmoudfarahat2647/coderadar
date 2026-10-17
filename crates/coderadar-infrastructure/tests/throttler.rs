//! Tests for the IPC throttler and streaming scan pipeline (Phase 3).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use coderadar_domain::{Finding, FindingCategory, RiskScore, Severity};
use coderadar_infrastructure::{
    scan_project, scan_project_streaming, FindingBatch, FindingThrottler,
};

#[test]
fn throttler_batches_findings_and_flushes_at_interval() {
    let collected_batches: Arc<Mutex<Vec<FindingBatch>>> = Arc::new(Mutex::new(Vec::new()));
    let collected_clone = Arc::clone(&collected_batches);

    let (sender, handle) =
        FindingThrottler::start(Duration::from_millis(50), move |batch: FindingBatch| {
            collected_clone.lock().unwrap().push(batch);
        });

    // Send 3 findings rapidly
    for i in 1..=3 {
        sender.send(
            Finding::new(
                format!("RULE-{i}"),
                format!("src/file_{i}.ts"),
                10 * i,
                10 * i + 5,
            )
            .with_severity(Severity::High)
            .with_category(FindingCategory::Security)
            .with_risk_score(RiskScore::new(0.75).unwrap()),
        );
    }

    // Wait 80 ms to ensure interval elapses and flushes
    thread::sleep(Duration::from_millis(80));

    // Send 2 more findings
    for i in 4..=5 {
        sender.send(
            Finding::new(
                format!("RULE-{i}"),
                format!("src/file_{i}.ts"),
                10 * i,
                10 * i + 5,
            )
            .with_severity(Severity::Low)
            .with_category(FindingCategory::Performance)
            .with_risk_score(RiskScore::new(0.25).unwrap()),
        );
    }

    // Drop sender to signal end of stream
    drop(sender);
    let total = handle.join();
    assert_eq!(total, 5);

    let batches = collected_batches.lock().unwrap().clone();
    assert!(!batches.is_empty(), "Should have at least one batch");

    let total_dispatched: usize = batches.iter().map(|b| b.count).sum();
    assert_eq!(total_dispatched, 5);

    // Verify last batch is marked is_last = true
    let last_batch = batches.last().unwrap();
    assert!(last_batch.is_last);

    // Verify polar projection worked
    let first_finding = &batches[0].findings[0];
    assert!((first_finding.radar_r - 0.75).abs() < f32::EPSILON);
    assert_eq!(first_finding.category, "security");
}

#[test]
fn scan_project_streaming_matches_batch_report() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/known-bad-repo");
    assert!(fixture_dir.exists(), "fixture repo must exist");

    let batch_report = scan_project(&fixture_dir).expect("scan_project should succeed");

    let streamed_findings_count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&streamed_findings_count);

    let streaming_report = scan_project_streaming(
        &fixture_dir,
        Duration::from_millis(20),
        move |batch: FindingBatch| {
            count_clone.fetch_add(batch.findings.len(), Ordering::SeqCst);
        },
    )
    .expect("scan_project_streaming should succeed");

    assert_eq!(
        streamed_findings_count.load(Ordering::SeqCst),
        15,
        "streaming should stream all 15 findings"
    );
    assert_eq!(
        batch_report.session.totals.total,
        streaming_report.session.totals.total
    );
    assert_eq!(
        batch_report.scan.files_scanned,
        streaming_report.scan.files_scanned
    );
}
