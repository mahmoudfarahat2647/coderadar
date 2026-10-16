use std::str::FromStr;

use coderadar_domain::{
    CodeChunk, Finding, FindingCategory, QualityScore, RiskScore, ScanMode, ScanSession, Severity,
};

#[test]
fn risk_score_rejects_out_of_range_and_non_finite() {
    assert!(RiskScore::new(1.5).is_err());
    assert!(RiskScore::new(-0.1).is_err());
    assert!(RiskScore::new(f32::NAN).is_err());
    assert!(RiskScore::new(f32::INFINITY).is_err());
    assert!(RiskScore::new(0.0).is_ok());
    assert!(RiskScore::new(1.0).is_ok());
}

#[test]
fn risk_score_saturating_clamps() {
    assert_eq!(RiskScore::saturating(5.0), RiskScore::ONE);
    assert_eq!(RiskScore::saturating(-2.0), RiskScore::ZERO);
    assert_eq!(RiskScore::saturating(f32::NAN), RiskScore::ZERO);
}

#[test]
fn severity_ordering_and_weight_are_monotonic() {
    let mut ordered = Severity::all().to_vec();
    ordered.sort();
    assert_eq!(
        ordered,
        vec![
            Severity::Clean,
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ]
    );
    assert!(Severity::Clean.weight() < Severity::Critical.weight());
    assert!(Severity::Critical.is_actionable());
    assert!(!Severity::Clean.is_actionable());
}

#[test]
fn severity_parses_case_insensitively() {
    assert_eq!(Severity::from_str("critical").unwrap(), Severity::Critical);
    assert_eq!(Severity::from_str(" High ").unwrap(), Severity::High);
    assert!(Severity::from_str("bogus").is_err());
}

#[test]
fn risk_score_from_severity_uses_weight() {
    assert_eq!(RiskScore::from(Severity::Critical), RiskScore::ONE);
    assert_eq!(RiskScore::from(Severity::Clean), RiskScore::ZERO);
}

#[test]
fn quality_score_is_the_inverse_of_risk() {
    let quality = QualityScore::from_risk(RiskScore::new(0.25).unwrap());
    assert!((quality.value() - 0.75).abs() < f32::EPSILON);
}

#[test]
fn category_tokens_match_manifest() {
    assert_eq!(FindingCategory::Security.as_str(), "security");
    assert_eq!(FindingCategory::CleanArchitecture.as_str(), "architecture");
    assert_eq!(FindingCategory::EdgeCases.as_str(), "edge_cases");
    assert_eq!(FindingCategory::Performance.as_str(), "performance");
    assert_eq!(FindingCategory::PullRequests.as_str(), "pull_requests");

    for category in FindingCategory::all() {
        assert_eq!(
            FindingCategory::from_str(category.as_str()).unwrap(),
            category
        );
        assert!(category.rule_prefix().len() >= 2);
    }
}

#[test]
fn finding_builder_is_deterministic_and_clamps_confidence() {
    let finding = Finding::new("SEC-INJ-001", "src/db.ts", 12, 14)
        .with_category(FindingCategory::Security)
        .with_severity(Severity::Critical)
        .with_confidence(4.2);

    assert_eq!(finding.id, "SEC-INJ-001:src/db.ts:12");
    assert_eq!(finding.line_span(), 3);
    assert!((finding.confidence - 1.0).abs() < f32::EPSILON);
    assert!(!finding.is_clean());
}

#[test]
fn scan_session_summary_counts_severities() {
    let mut session = ScanSession::new("scan-1", "/repo", ScanMode::FullScan, 1_000);
    session.finished_at_unix_ms = Some(2_000);
    session.push(Finding::new("SEC-INJ-001", "a.ts", 1, 1).with_severity(Severity::Critical));
    session.push(Finding::new("SEC-XSS-004", "b.ts", 1, 1).with_severity(Severity::High));
    session.push(Finding::new("SEC-XSS-004", "c.ts", 1, 1).with_severity(Severity::High));

    let summary = session.summary();
    assert_eq!(summary.total_findings, 3);
    assert_eq!(summary.critical, 1);
    assert_eq!(summary.high, 2);
    assert_eq!(summary.medium, 0);
    assert_eq!(summary.finished_at_unix_ms, 2_000);
}

#[test]
fn code_chunk_reports_bloat() {
    let small = CodeChunk::new("a.ts", "typescript", 1, 10, "x");
    assert!(!small.exceeds_bloat_guard());
    assert_eq!(small.line_count(), 10);

    let wide = CodeChunk::new("a.ts", "typescript", 1, 301, "");
    assert!(wide.exceeds_bloat_guard());
}
