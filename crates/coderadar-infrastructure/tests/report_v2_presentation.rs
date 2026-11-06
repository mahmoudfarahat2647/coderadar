use coderadar_domain::{ScanMode, ScanSession};
use coderadar_infrastructure::{
    markdown_from_report, scan_project, LayerMapSource, RuleManifest, ScanOutcome, ScanReport,
};

#[test]
fn empty_scan_reports_unavailable_score_and_unknown_coverage() {
    let outcome = ScanOutcome {
        session: ScanSession::new("empty", ".", ScanMode::FullScan, 0),
        files_scanned: 0,
        ast_chunks: 0,
        module_chunks: 0,
        pull_request: None,
    };
    let report = ScanReport::from_scan(
        &outcome,
        &RuleManifest::embedded().unwrap(),
        LayerMapSource::Fallback,
        0,
    );
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema_version"], 2);
    assert!(json["scan"]["compliance_percent"].is_null());
    assert!(json["scan"]["coverage"].is_null());
    assert!(json["scan"]["completion"].is_null());
    let markdown = markdown_from_report(&report);
    assert!(markdown.contains("| Compliance | Unavailable |"));
    assert!(markdown.contains("| Completion | Unknown |"));
    assert!(markdown.contains("Coverage not recorded"));
    assert!(!markdown.contains("passed cleanly"));
}

#[test]
fn real_scans_propagate_coverage_and_scores_to_json_and_markdown() {
    let directory = tempfile::TempDir::new().unwrap();
    std::fs::write(directory.path().join("unsupported.py"), "print('hello')\n").unwrap();
    let report = scan_project(directory.path()).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert!(json["scan"]["compliance_percent"].is_null());
    assert_eq!(json["scan"]["coverage"]["unsupported"], 1);
    assert_eq!(json["scan"]["coverage"]["analyzed"], 0);
    assert_eq!(json["scan"]["coverage"]["exclusions_complete"], false);
    let markdown = markdown_from_report(&report);
    assert!(markdown.contains("| Compliance | Unavailable |"));
    assert!(markdown.contains("| Unsupported | 1 |"));
    assert!(markdown.contains("Excluded (incomplete accounting)"));

    std::fs::write(
        directory.path().join("supported.ts"),
        "export const value = 1;\n",
    )
    .unwrap();
    let report = scan_project(directory.path()).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["scan"]["compliance_percent"], 100);
    assert_eq!(json["scan"]["coverage"]["unsupported"], 1);
    assert_eq!(json["scan"]["coverage"]["analyzed"], 1);
    let markdown = markdown_from_report(&report);
    assert!(markdown.contains("| Compliance | 100 % |"));
    assert!(markdown.contains("| Analyzed | 1 |"));
}

#[test]
fn history_dto_preserves_legacy_unknown_coverage_and_nullable_scores() {
    let session = ScanSession::new("legacy", ".", ScanMode::FullScan, 0);
    let mut summary = session.summary();
    summary.compliance_percent = Some(80);
    let legacy = coderadar_adapters::HistoryPointDTO::from_summary(&summary);
    let json = serde_json::to_value(&legacy).unwrap();
    assert_eq!(json["compliance_percent"], 80);
    assert!(json["coverage"].is_null());
    summary.compliance_percent = None;
    let json =
        serde_json::to_value(coderadar_adapters::HistoryPointDTO::from_summary(&summary)).unwrap();
    assert!(json["compliance_percent"].is_null());
}

#[test]
fn completed_report_findings_have_desktop_radar_coordinates() {
    let directory = tempfile::TempDir::new().unwrap();
    std::fs::write(
        directory.path().join("secret.ts"),
        "const password = 'AKIAIOSFODNN7EXAMPLE';\n",
    )
    .unwrap();
    let report = scan_project(directory.path()).unwrap();
    assert!(!report.findings.is_empty());
    let finding = &report.findings[0];
    assert_eq!(finding.radar_r, finding.risk_score);
    assert!((0.0..360.0).contains(&finding.radar_theta));
    let json = serde_json::to_value(finding).unwrap();
    assert!(json["radar_r"].is_number());
    assert!(json["radar_theta"].is_number());
}

#[test]
fn incomplete_scan_reports_source_identities_and_limitations() {
    let mut session = ScanSession::new("partial", ".", ScanMode::FullScan, 0).with_files_scanned(1);
    session.coverage = Some(coderadar_domain::ScanCoverage {
        discovered: 2,
        supported: 2,
        analyzed: 1,
        incomplete: 1,
        ..Default::default()
    });
    session.analyzed_sources = Some([("src/a.ts".to_string(), "abc123".to_string())].into());
    session.consumed_sources = Some(
        [
            ("src/a.ts".to_string(), "abc123".to_string()),
            ("src/b.ts".to_string(), "def456".to_string()),
        ]
        .into(),
    );
    let outcome = ScanOutcome {
        session,
        files_scanned: 1,
        ast_chunks: 1,
        module_chunks: 1,
        pull_request: None,
    };
    let report = ScanReport::from_scan(
        &outcome,
        &RuleManifest::embedded().unwrap(),
        LayerMapSource::Fallback,
        0,
    );
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["scan"]["completion"], "partial");
    assert_eq!(json["scan"]["analyzed_sources"]["src/a.ts"], "abc123");
    assert!(json["scan"]["analyzed_sources"].get("src/b.ts").is_none());
    assert_eq!(json["scan"]["consumed_sources"]["src/b.ts"], "def456");
    let markdown = markdown_from_report(&report);
    assert!(markdown.contains("| Completion | partial |"));
    assert!(markdown.contains("### Analyzed source identities"));
    assert!(markdown.contains("### Consumed source identities"));
    assert!(markdown.contains("src/b.ts"));
    assert!(markdown.contains("def456"));
    assert!(markdown.contains("src/a.ts"));
    assert!(markdown.contains("abc123"));
    assert!(markdown.contains("incomplete analysis"));
}

#[test]
fn zero_incomplete_does_not_prove_completion_and_unknown_sources_stay_null() {
    let mut session = ScanSession::new("unknown", ".", ScanMode::FullScan, 0);
    session.coverage = Some(Default::default());
    let outcome = ScanOutcome {
        session,
        files_scanned: 0,
        ast_chunks: 0,
        module_chunks: 0,
        pull_request: None,
    };
    let report = ScanReport::from_scan(
        &outcome,
        &RuleManifest::embedded().unwrap(),
        LayerMapSource::Fallback,
        0,
    );
    let json = serde_json::to_value(&report).unwrap();
    assert!(json["scan"]["completion"].is_null());
    assert!(json["scan"]["analyzed_sources"].is_null());
    assert!(json["scan"]["consumed_sources"].is_null());
    assert!(json["scan"]["coverage"]["exclusions_complete"].is_null());
    assert!(markdown_from_report(&report).contains("Excluded (accounting unknown)"));
    assert!(markdown_from_report(&report).contains("Source identities not recorded"));
}
