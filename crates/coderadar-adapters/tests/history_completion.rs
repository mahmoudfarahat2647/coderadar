use coderadar_adapters::HistoryPointDTO;
use coderadar_domain::{ScanMode, ScanSession, ScanUsage};

#[test]
fn history_preserves_recorded_and_unknown_completion() {
    let mut session = ScanSession::new("id", "root", ScanMode::FullScan, 1);
    session.finished_at_unix_ms = Some(2);
    for completion in [None, Some("partial".to_string())] {
        session.completion = completion.clone();
        let dto = HistoryPointDTO::from_summary(&session.summary());
        assert_eq!(dto.completion, completion);
    }
}

#[test]
fn history_preserves_mixed_fallback_and_unknown_usage() {
    let mut session = ScanSession::new("id", "root", ScanMode::FullScan, 1);
    session.finished_at_unix_ms = Some(2);
    assert!(HistoryPointDTO::from_summary(&session.summary())
        .usage
        .is_none());
    session.usage = Some(ScanUsage {
        primary_batches: 3,
        fallback_batches: 2,
        fallback_failures: 1,
    });
    let usage = HistoryPointDTO::from_summary(&session.summary())
        .usage
        .unwrap();
    assert_eq!(
        (
            usage.primary_batches,
            usage.fallback_batches,
            usage.fallback_failures
        ),
        (3, 2, 1)
    );
}
