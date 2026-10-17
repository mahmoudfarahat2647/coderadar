//! End-to-end scan tests over the curated `known-bad-repo` fixture.
//!
//! The fixture fixes the exact expected finding set
//! (`expected-findings.json`): the scan must produce every listed
//! `(rule_id, file, line_start)` triple and nothing else. A single extra or
//! missing finding fails the exact-equality assertion.

use std::path::Path;

use coderadar_infrastructure::{scan_project, ScanReport};
use serde::Deserialize;

/// Path of the curated fixture repository.
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/known-bad-repo");

#[derive(Deserialize)]
struct ExpectedFindings {
    findings: Vec<ExpectedFinding>,
}

#[derive(Deserialize)]
struct ExpectedFinding {
    rule_id: String,
    file: String,
    line_start: u32,
}

fn expected_triples() -> Vec<(String, String, u32)> {
    let raw = std::fs::read_to_string(Path::new(FIXTURE).join("expected-findings.json"))
        .expect("expected-findings.json is readable");
    let expected: ExpectedFindings =
        serde_json::from_str(&raw).expect("expected-findings.json is valid JSON");
    expected
        .findings
        .into_iter()
        .map(|finding| (finding.rule_id, finding.file, finding.line_start))
        .collect()
}

fn actual_triples(report: &ScanReport) -> Vec<(String, String, u32)> {
    report
        .findings
        .iter()
        .map(|finding| {
            (
                finding.rule_id.clone(),
                finding.file.clone(),
                finding.line_start,
            )
        })
        .collect()
}

#[test]
fn fixture_scan_matches_expected_findings_exactly() {
    let report = scan_project(Path::new(FIXTURE)).expect("fixture scan succeeds");

    let mut expected = expected_triples();
    let mut actual = actual_triples(&report);
    expected.sort();
    actual.sort();
    assert_eq!(
        actual, expected,
        "finding set diverged from expected-findings.json"
    );

    assert_eq!(actual.len(), 15, "the fixture pins exactly 15 findings");
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.file == "ignored.ts"),
        "gitignored files must never be scanned"
    );
    assert_eq!(report.layer_map_source, "config_file");
    assert_eq!(report.session.mode, "full_scan");
    assert_eq!(report.scan.files_scanned, 12);
    assert_eq!(report.scan.module_chunks, 12);
    assert!(
        report.scan.ast_chunks >= 12,
        "every file yields at least one AST window, got {}",
        report.scan.ast_chunks
    );

    let totals = &report.session.totals;
    assert_eq!(totals.total, report.findings.len());
    assert_eq!(
        totals.critical + totals.high + totals.medium + totals.low,
        totals.total,
        "severity totals must add up"
    );
}

#[test]
fn repeated_scans_are_deterministic() {
    let first = scan_project(Path::new(FIXTURE)).expect("first scan succeeds");
    let second = scan_project(Path::new(FIXTURE)).expect("second scan succeeds");

    let first_findings = serde_json::to_value(&first.findings).unwrap();
    let second_findings = serde_json::to_value(&second.findings).unwrap();
    assert_eq!(
        first_findings, second_findings,
        "findings differ across runs"
    );

    assert_eq!(first.scan.files_scanned, second.scan.files_scanned);
    assert_eq!(first.scan.ast_chunks, second.scan.ast_chunks);
    assert_eq!(first.scan.module_chunks, second.scan.module_chunks);
    assert_eq!(first.session.totals.total, second.session.totals.total);
}

#[test]
fn clean_project_without_config_falls_back_and_finds_nothing() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    std::fs::write(
        temp.path().join("clean.ts"),
        "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
    )
    .expect("clean source is writable");

    let report = scan_project(temp.path()).expect("clean scan succeeds");

    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    assert_eq!(report.layer_map_source, "fallback");
    assert_eq!(report.scan.files_scanned, 1);
    assert_eq!(report.session.totals.total, 0);
}
