//! Coverage and compliance are retained by session summaries.
use coderadar_domain::{ScanCoverage, ScanMode, ScanSession};

#[test]
fn unknown_and_empty_coverage_have_no_compliance_score() {
    let mut session = ScanSession::new("id", "root", ScanMode::FullScan, 0);
    assert_eq!(session.coverage, None);
    assert_eq!(session.compliance_percent(), None);
    session.coverage = Some(ScanCoverage::default());
    assert_eq!(session.summary().coverage, session.coverage);
    assert_eq!(session.summary().compliance_percent, None);
}

#[test]
fn nonempty_clean_session_has_a_score() {
    let session = ScanSession::new("id", "root", ScanMode::FullScan, 0).with_files_scanned(2);
    assert_eq!(session.compliance_percent(), Some(100));
    assert_eq!(session.summary().compliance_percent, Some(100));
}
