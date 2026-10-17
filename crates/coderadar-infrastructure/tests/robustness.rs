//! Adversarial edge-case suite for the Phase 2 scan engine.
//!
//! Everything here drives the public boundary — [`scan_project`] or the real
//! `coderadar` binary — and asserts the *contract* (documented in
//! `src/scanner.rs`, `src/layer_map.rs`, `src/report.rs`, `src/mock.rs` and the
//! fixture's `expected-findings.json`), never an implementation detail. The
//! primary invariant is the security one: raw secret values must never reach
//! the serialized report.
//!
//! Tests that document a defect are marked `#[ignore = "BUG: ..."]` so the
//! suite stays green while the defect is tracked separately.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use coderadar_infrastructure::{scan_project, ScanReport, MAX_FILE_BYTES, MAX_LINE_LENGTH};
use serde_json::Value;
use tempfile::TempDir;

/// The compiled CLI under test.
const BIN: &str = env!("CARGO_BIN_EXE_coderadar");
/// Path of the curated fixture repository.
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/known-bad-repo");

/// Write `contents` to `root/relative`, creating parent directories.
fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory is creatable");
    }
    fs::write(path, contents).expect("test file is writable");
}

/// The `(rule_id, file, line_start)` triples of a report.
fn triples(report: &ScanReport) -> BTreeSet<(String, String, u32)> {
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

/// Sorted line numbers reported for one rule.
fn rule_lines(report: &ScanReport, rule_id: &str) -> Vec<u32> {
    let mut lines: Vec<u32> = report
        .findings
        .iter()
        .filter(|finding| finding.rule_id == rule_id)
        .map(|finding| finding.line_start)
        .collect();
    lines.sort_unstable();
    lines
}

/// Scan the curated fixture repository.
fn scan_fixture() -> ScanReport {
    scan_project(Path::new(FIXTURE)).expect("fixture scan succeeds")
}

/// Run the compiled CLI.
fn run_cli(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("coderadar binary is runnable")
}

// ---------------------------------------------------------------------------
// 1. Secret masking, end to end.
// ---------------------------------------------------------------------------

#[test]
fn fixture_report_never_leaks_raw_secret_values() {
    const SECRETS: [&str; 3] = [
        "AKIAIOSFODNN7EXAMPLE",
        "sk_live_51H8xQ2BkR7dKw9mZq4Lt3f",
        "Zq4Lm9xT2vB8nR5s",
    ];

    // Guard the guard: the fixture must actually contain the secrets, or this
    // test would pass vacuously.
    let fixture_source = fs::read_to_string(Path::new(FIXTURE).join("src/config/secrets.ts"))
        .expect("fixture secrets file is readable");
    for secret in SECRETS {
        assert!(
            fixture_source.contains(secret),
            "fixture no longer contains {secret}"
        );
    }

    let report = scan_fixture();
    assert_eq!(
        rule_lines(&report, "SEC-TOK-002"),
        vec![1, 2, 3],
        "the three fixture secrets must be detected: {:#?}",
        report.findings
    );

    // Walk the whole serialized document as raw text: no string field
    // (description, title, id, path, ...) may carry a raw secret.
    let json = serde_json::to_string(&report).expect("report serializes");
    for secret in SECRETS {
        assert!(
            !json.contains(secret),
            "raw secret `{secret}` leaked into the JSON report"
        );
    }
}

// ---------------------------------------------------------------------------
// 2-3. Degenerate repositories.
// ---------------------------------------------------------------------------

#[test]
fn empty_repo_scans_to_zero_findings() {
    let temp = TempDir::new().expect("temp dir");
    let report = scan_project(temp.path()).expect("empty scan succeeds");

    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    assert_eq!(report.scan.files_scanned, 0);
    assert_eq!(report.scan.module_chunks, 0);
    assert_eq!(report.session.totals.total, 0);
    assert_eq!(report.layer_map_source, "fallback");
}

#[test]
fn repo_without_supported_files_is_ignored() {
    let temp = TempDir::new().expect("temp dir");
    write(
        temp.path(),
        "script.py",
        "url = \"http://api.example.com\"\nkey = \"AKIAIOSFODNN7EXAMPLE\"\n",
    );
    write(
        temp.path(),
        "main.rs",
        "const URL: &str = \"http://api.example.com\";\nconst KEY: &str = \"AKIAIOSFODNN7EXAMPLE\";\n",
    );

    let report = scan_project(temp.path()).expect("scan succeeds");

    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    assert_eq!(report.scan.files_scanned, 0, "no TS/JS file was written");
}

// ---------------------------------------------------------------------------
// 4. `.coderadar.json` provenance contract.
// ---------------------------------------------------------------------------

#[test]
fn layer_map_source_follows_the_documented_contract() {
    // Malformed JSON -> fallback conventions.
    let malformed = TempDir::new().expect("temp dir");
    write(malformed.path(), ".coderadar.json", "{ oops");
    let report = scan_project(malformed.path()).expect("malformed config never aborts a scan");
    assert_eq!(report.layer_map_source, "fallback");

    // Valid JSON with an unknown layer key -> still config_file; unknown keys
    // are ignored and missing layers take their conventional globs.
    let unknown = TempDir::new().expect("temp dir");
    write(
        unknown.path(),
        ".coderadar.json",
        r#"{"layers":{"mystery":["m/**"]}}"#,
    );
    write(
        unknown.path(),
        "src/domain/x.ts",
        "import express from \"express\";\n",
    );
    let report = scan_project(unknown.path()).expect("valid config scans");
    assert_eq!(report.layer_map_source, "config_file");
    assert!(
        triples(&report).contains(&("ARCH-DEP-001".to_string(), "src/domain/x.ts".to_string(), 1)),
        "missing layers must fall back to conventional globs: {:#?}",
        report.findings
    );

    // Empty `layers` object -> config_file, conventional globs for all layers.
    let empty = TempDir::new().expect("temp dir");
    write(empty.path(), ".coderadar.json", r#"{"layers":{}}"#);
    write(
        empty.path(),
        "src/domain/x.ts",
        "import express from \"express\";\n",
    );
    let report = scan_project(empty.path()).expect("empty config scans");
    assert_eq!(report.layer_map_source, "config_file");
    assert!(
        triples(&report).contains(&("ARCH-DEP-001".to_string(), "src/domain/x.ts".to_string(), 1)),
        "empty layers object must keep conventional globs: {:#?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// 5-7. Source-shape robustness: syntax errors, CRLF, Unicode.
// ---------------------------------------------------------------------------

#[test]
fn syntax_error_file_still_yields_findings() {
    let temp = TempDir::new().expect("temp dir");
    write(
        temp.path(),
        "broken.ts",
        "}{ )(\nconst url = \"http://api.example.com\";\nconst key = \"AKIAIOSFODNN7EXAMPLE\";\n",
    );

    let report = scan_project(temp.path()).expect("unparseable file never aborts a scan");

    assert_eq!(report.scan.files_scanned, 1);
    let actual = triples(&report);
    assert!(
        actual.contains(&("SEC-TLS-005".to_string(), "broken.ts".to_string(), 2)),
        "whole-file rule lost its line: {:#?}",
        report.findings
    );
    assert!(
        actual.contains(&("SEC-TOK-002".to_string(), "broken.ts".to_string(), 3)),
        "fallback windowing lost the secret line: {:#?}",
        report.findings
    );
}

#[test]
fn crlf_line_endings_do_not_shift_line_numbers() {
    let temp = TempDir::new().expect("temp dir");
    write(
        temp.path(),
        "crlf.ts",
        "const secure = \"https://secure.example.com\";\r\nconst base = \"http://api.example.com/v1\";\r\nconst agent = { rejectUnauthorized: false };\r\n",
    );

    let report = scan_project(temp.path()).expect("CRLF scan succeeds");

    assert_eq!(
        rule_lines(&report, "SEC-TLS-005"),
        vec![2, 3],
        "CRLF must not shift or swallow lines: {:#?}",
        report.findings
    );
    assert_eq!(report.findings.len(), 2, "{:#?}", report.findings);
}

#[test]
fn unicode_lines_do_not_shift_line_numbers() {
    let temp = TempDir::new().expect("temp dir");
    write(
        temp.path(),
        "unicode.ts",
        concat!(
            "const greeting = \"こんにちは 🚀 café\";\n",
            "const label = \"日本語のテキスト\";\n",
            "const shield = \"🛡️🔒\";\n",
            "const key = \"AKIAIOSFODNN7EXAMPLE\";\n",
        ),
    );

    let report = scan_project(temp.path()).expect("unicode scan succeeds");

    assert_eq!(report.findings.len(), 1, "{:#?}", report.findings);
    assert_eq!(report.findings[0].rule_id, "SEC-TOK-002");
    assert_eq!(
        report.findings[0].line_start, 4,
        "multibyte characters shifted the line number"
    );
}

// ---------------------------------------------------------------------------
// 8-9. Bloat-guard boundaries.
// ---------------------------------------------------------------------------

#[test]
fn line_length_boundary_is_exactly_one_thousand_characters() {
    assert_eq!(MAX_LINE_LENGTH, 1000);

    fn padded_line(total_chars: usize) -> String {
        let prefix_and_suffix = "const note = \"\";".len();
        let line = format!(
            "const note = \"{}\";",
            "a".repeat(total_chars - prefix_and_suffix)
        );
        assert_eq!(line.chars().count(), total_chars);
        line
    }

    let allowed = TempDir::new().expect("temp dir");
    write(
        allowed.path(),
        "note.ts",
        &format!("{}\n", padded_line(1000)),
    );
    let report = scan_project(allowed.path()).expect("scan succeeds");
    assert_eq!(
        report.scan.files_scanned, 1,
        "a line of exactly {MAX_LINE_LENGTH} characters is allowed"
    );

    let skipped = TempDir::new().expect("temp dir");
    write(
        skipped.path(),
        "note.ts",
        &format!("{}\n", padded_line(1001)),
    );
    let report = scan_project(skipped.path()).expect("scan succeeds");
    assert_eq!(
        report.scan.files_scanned,
        0,
        "a line of {} characters must skip the file",
        MAX_LINE_LENGTH + 1
    );
}

#[test]
fn file_size_boundary_is_exactly_one_megabyte() {
    assert_eq!(MAX_FILE_BYTES, 1024 * 1024);

    fn padded_file(target: u64) -> String {
        let mut content = String::new();
        let line = format!("// {}\n", "p".repeat(97));
        while content.len() as u64 + line.len() as u64 <= target {
            content.push_str(&line);
        }
        let remainder = (target - content.len() as u64) as usize;
        if remainder >= 4 {
            content.push_str("// ");
            content.push_str(&"q".repeat(remainder - 3));
        } else if remainder > 0 {
            content.push_str(&"r".repeat(remainder));
        }
        assert_eq!(content.len() as u64, target, "padding must hit the target");
        assert!(
            content
                .lines()
                .all(|line| line.chars().count() <= MAX_LINE_LENGTH),
            "no padding line may trip the line guard"
        );
        content
    }

    let allowed = TempDir::new().expect("temp dir");
    write(allowed.path(), "huge.ts", &padded_file(MAX_FILE_BYTES));
    let report = scan_project(allowed.path()).expect("1 MB scan succeeds");
    assert_eq!(
        report.scan.files_scanned, 1,
        "a file of exactly {MAX_FILE_BYTES} bytes is scanned"
    );
    assert!(report.findings.is_empty(), "{:#?}", report.findings);

    let too_big = TempDir::new().expect("temp dir");
    write(too_big.path(), "huge.ts", &padded_file(MAX_FILE_BYTES + 1));
    let report = scan_project(too_big.path()).expect("oversized scan succeeds");
    assert_eq!(
        report.scan.files_scanned,
        0,
        "a file of {} bytes must be skipped",
        MAX_FILE_BYTES + 1
    );
}

// ---------------------------------------------------------------------------
// 10. Deep nesting.
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_paths_stay_relative_and_slash_separated() {
    let temp = TempDir::new().expect("temp dir");
    let mut relative = String::from("src/domain");
    for level in 1..=20 {
        relative.push_str(&format!("/l{level:02}"));
    }
    relative.push_str("/deep.ts");
    write(
        temp.path(),
        &relative,
        "const base = \"http://api.example.com\";\n",
    );

    let report = scan_project(temp.path()).expect("deep scan succeeds");

    let finding = report
        .findings
        .iter()
        .find(|finding| finding.rule_id == "SEC-TLS-005")
        .unwrap_or_else(|| panic!("deep finding missing: {:#?}", report.findings));
    assert_eq!(finding.file, relative);
    assert!(!finding.file.contains('\\'), "{}", finding.file);
    assert!(Path::new(&finding.file).is_relative(), "{}", finding.file);
    assert_eq!(finding.line_start, 1);
}

// ---------------------------------------------------------------------------
// 11. Per-line uniqueness of `(rule_id, file, line_start)`.
// ---------------------------------------------------------------------------

#[test]
fn duplicate_rule_file_line_triples_never_appear() {
    let fixture = scan_fixture();
    assert_eq!(
        triples(&fixture).len(),
        fixture.findings.len(),
        "fixture report contains duplicate triples: {:#?}",
        fixture.findings
    );

    // Two occurrences on different lines are two findings.
    let twice = TempDir::new().expect("temp dir");
    write(
        twice.path(),
        "twice.ts",
        concat!(
            "const a = { rejectUnauthorized: false };\n",
            "const b = 1;\n",
            "const c = { rejectUnauthorized: false };\n",
        ),
    );
    let report = scan_project(twice.path()).expect("scan succeeds");
    assert_eq!(
        rule_lines(&report, "SEC-TLS-005"),
        vec![1, 3],
        "{:#?}",
        report.findings
    );

    // Two triggers on the same line collapse to exactly one finding.
    let same_line = TempDir::new().expect("temp dir");
    write(
        same_line.path(),
        "same_line.ts",
        "const opts = { rejectUnauthorized: false, base: \"http://api.example.com\" };\n",
    );
    let report = scan_project(same_line.path()).expect("scan succeeds");
    assert_eq!(
        rule_lines(&report, "SEC-TLS-005"),
        vec![1],
        "one line must yield at most one SEC-TLS-005 finding: {:#?}",
        report.findings
    );
    assert_eq!(triples(&report).len(), report.findings.len());
}

// ---------------------------------------------------------------------------
// 12-13. CLI boundary behaviour.
// ---------------------------------------------------------------------------

#[test]
fn cli_rejects_a_file_path_with_exit_code_one() {
    let temp = TempDir::new().expect("temp dir");
    let file = temp.path().join("single.ts");
    fs::write(&file, "export const x = 1;\n").expect("file is writable");

    let output = run_cli(&["scan", file.to_str().expect("temp path is UTF-8")]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "file path is a runtime failure"
    );
    assert!(output.stdout.is_empty(), "failures never write stdout");
    assert!(
        !output.stderr.is_empty(),
        "failures must explain themselves on stderr"
    );
}

#[test]
fn cli_empty_directory_reports_zero_findings() {
    let temp = TempDir::new().expect("temp dir");

    let output = run_cli(&["scan", temp.path().to_str().expect("temp path is UTF-8")]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout parses as JSON");
    assert_eq!(value["findings"].as_array().map(Vec::len), Some(0));
    assert_eq!(value["scan"]["files_scanned"], 0);
    assert_eq!(value["session"]["totals"]["total"], 0);
}

// ---------------------------------------------------------------------------
// 14-15. Determinism and output-mode equivalence.
// ---------------------------------------------------------------------------

#[test]
fn three_repeated_scans_produce_identical_findings() {
    let runs: Vec<Value> = (0..3)
        .map(|_| {
            let report = scan_fixture();
            serde_json::to_value(&report.findings).expect("findings serialize")
        })
        .collect();

    assert_eq!(runs[0], runs[1], "runs 1 and 2 differ");
    assert_eq!(runs[1], runs[2], "runs 2 and 3 differ");
    assert_eq!(
        runs[0].as_array().map(Vec::len),
        Some(15),
        "fixture must not silently change size"
    );
}

#[test]
fn pretty_and_compact_output_carry_identical_findings() {
    let compact = run_cli(&["scan", FIXTURE]);
    let pretty = run_cli(&["scan", FIXTURE, "--pretty"]);
    assert_eq!(compact.status.code(), Some(0));
    assert_eq!(pretty.status.code(), Some(0));

    let compact_value: Value =
        serde_json::from_slice(&compact.stdout).expect("compact stdout parses");
    let pretty_value: Value = serde_json::from_slice(&pretty.stdout).expect("pretty stdout parses");

    assert_eq!(compact_value["findings"], pretty_value["findings"]);
}

// ---------------------------------------------------------------------------
// 16. Negative controls: legal patterns must stay silent.
// ---------------------------------------------------------------------------

#[test]
fn legal_patterns_produce_no_findings() {
    let temp = TempDir::new().expect("temp dir");

    write(
        temp.path(),
        "secure_urls.ts",
        concat!(
            "export const api = \"https://api.example.com/v1\";\n",
            "export const dev = \"http://localhost:3000/api\";\n",
            "export const loopback = \"http://127.0.0.1/dev\";\n",
        ),
    );
    write(
        temp.path(),
        "tls_ok.ts",
        concat!(
            "export const agent = { rejectUnauthorized: true, minVersion: \"TLSv1.3\" };\n",
            "export const directive = \"ssl_protocols TLSv1.2 TLSv1.3;\";\n",
        ),
    );
    write(
        temp.path(),
        "src/infrastructure/db.ts",
        concat!(
            "import { Order } from \"../domain/order\";\n",
            "export const load = (): Order | null => null;\n",
        ),
    );
    write(
        temp.path(),
        "low_entropy.ts",
        concat!(
            "export const token = \"aaaaaaaaaaaaaaaa\";\n",
            "export const apiKey = \"aaaaaaaaaaaaaaaaaaaaaaaa\";\n",
        ),
    );
    write(
        temp.path(),
        "fetch_ok.ts",
        concat!(
            "const controller = new AbortController();\n",
            "\n",
            "export function load(): Promise<void> {\n",
            "  return fetch(\"https://api.example.com/data\", { signal: controller.signal })\n",
            "    .then(handleResult)\n",
            "    .catch(reportError);\n",
            "}\n",
        ),
    );
    write(
        temp.path(),
        "src/domain/order.ts",
        "export interface Order { id: string }\n",
    );

    let report = scan_project(temp.path()).expect("clean scan succeeds");

    assert!(
        report.findings.is_empty(),
        "legal patterns produced false positives: {:#?}",
        report.findings
    );
    assert_eq!(report.scan.files_scanned, 6);
}

// ---------------------------------------------------------------------------
// 17. The documented shape of the fetch timeout heuristic.
// ---------------------------------------------------------------------------

#[test]
fn bare_fetch_with_catch_fires_only_the_documented_timeout_rule() {
    let temp = TempDir::new().expect("temp dir");
    write(
        temp.path(),
        "fetch_catch_only.ts",
        concat!(
            "export function load(): Promise<unknown> {\n",
            "  return fetch(\"https://api.example.com/data\").catch(reportError);\n",
            "}\n",
        ),
    );

    let report = scan_project(temp.path()).expect("scan succeeds");

    // `EDGE-PRM-002` documents that any `.catch(` in the chunk satisfies its
    // rejection-handling guard, so the promise rule stays silent here.
    assert!(
        !triples(&report)
            .iter()
            .any(|(rule_id, _, _)| rule_id == "EDGE-PRM-002"),
        "EDGE-PRM-002 fired despite `.catch(`: {:#?}",
        report.findings
    );

    // `EDGE-TMO-004`'s documented trigger (`src/mock.rs`: "fetch/axios call
    // without timeout, signal or abort in the chunk") does fire: a fetch with
    // catch-only handling still has no timeout. This pins the documented mock
    // matrix; it is not a regression.
    assert_eq!(
        report.findings.len(),
        1,
        "expected exactly the timeout finding: {:#?}",
        report.findings
    );
    assert_eq!(rule_lines(&report, "EDGE-TMO-004"), vec![2]);
}

// ---------------------------------------------------------------------------
// 18. Import resolution: root cap and config-driven layer map.
// ---------------------------------------------------------------------------

#[test]
fn import_escape_is_capped_and_config_layer_map_drives_findings() {
    let temp = TempDir::new().expect("temp dir");
    write(
        temp.path(),
        ".coderadar.json",
        r#"{"layers":{"domain":["src/domain/**"],"infrastructure":["src/infrastructure/**"]}}"#,
    );
    write(
        temp.path(),
        "src/domain/escape.ts",
        "import { x } from \"../../../../outside\";\n",
    );
    write(
        temp.path(),
        "src/domain/leak.ts",
        "import { db } from \"../infrastructure/db\";\n",
    );

    let report = scan_project(temp.path()).expect("scan succeeds");

    assert_eq!(report.layer_map_source, "config_file");
    assert_eq!(
        triples(&report),
        BTreeSet::from([(
            "ARCH-DEP-001".to_string(),
            "src/domain/leak.ts".to_string(),
            1
        )]),
        "escapes must cap at the root and only outer-layer relative imports may fire: {:#?}",
        report.findings
    );
}
