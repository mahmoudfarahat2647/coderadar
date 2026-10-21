//! Integration tests that drive the compiled `coderadar` binary end to end.
//!
//! Exit-code contract (`src/bin/coderadar.rs`): `0` scan completed (with or
//! without findings), `1` runtime failure, `2` usage error. Usage errors write
//! to stderr and leave stdout untouched; successful scans print only JSON.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_coderadar");
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/known-bad-repo");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("coderadar binary is runnable")
}

fn stdout_string(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

#[test]
fn compact_scan_prints_one_line_of_json_with_fifteen_findings() {
    let output = run(&["scan", FIXTURE]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = stdout_string(&output);
    let trimmed = stdout.trim();
    assert!(
        !trimmed.contains('\n'),
        "compact output must be a single line"
    );

    let value: serde_json::Value = serde_json::from_str(trimmed).expect("stdout parses as JSON");
    assert_eq!(value["tool"]["name"], "coderadar");
    assert_eq!(value["findings"].as_array().unwrap().len(), 15);
}

#[test]
fn pretty_scan_prints_indented_json() {
    let output = run(&["scan", FIXTURE, "--pretty"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = stdout_string(&output);
    assert!(stdout.contains('\n'), "pretty output must be multiline");

    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout parses as JSON");
    assert_eq!(value["tool"]["name"], "coderadar");
    assert_eq!(value["findings"].as_array().unwrap().len(), 15);
}

#[test]
fn missing_path_is_a_usage_error() {
    let output = run(&["scan"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "usage errors never write stdout");
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage"));
}

#[test]
fn unknown_flag_is_a_usage_error() {
    let output = run(&["scan", FIXTURE, "--frobnicate"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "usage errors never write stdout");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown flag"), "stderr: {stderr}");
    assert!(stderr.contains("usage"), "stderr: {stderr}");
}

#[test]
fn nonexistent_directory_is_a_runtime_failure() {
    let missing = Path::new(FIXTURE).join("definitely-not-here");
    let output = run(&["scan", missing.to_str().expect("fixture path is UTF-8")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "failures never write stdout");
    assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
}

#[test]
fn help_prints_usage_on_stdout() {
    let output = run(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout_string(&output);
    assert!(stdout.contains("usage"), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn markdown_scan_prints_the_markdown_report() {
    let output = run(&["scan", FIXTURE, "--markdown"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = stdout_string(&output);
    assert!(stdout.starts_with("# CodeRadar Studio — Scan Report"));
    assert!(stdout.contains("## Executive summary"));
    assert!(stdout.contains("## Findings by category"));
    assert!(stdout.contains("| Findings | 15 |"));
}

#[test]
fn the_report_command_writes_a_markdown_file() {
    let directory = tempfile::TempDir::new().expect("temp dir");
    let target = directory.path().join("report.md");

    let output = run(&[
        "report",
        FIXTURE,
        "--out",
        target.to_str().expect("path is UTF-8"),
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = std::fs::read_to_string(&target).expect("the report is readable");
    assert!(written.starts_with("# CodeRadar Studio — Scan Report"));
    assert!(written.contains("| Findings | 15 |"));
    assert!(
        stdout_string(&output).contains("report written to"),
        "the command confirms the written path"
    );
}

#[test]
fn the_report_command_defaults_to_report_md() {
    let directory = tempfile::TempDir::new().expect("temp dir");

    let output = Command::new(BIN)
        .args(["report", FIXTURE])
        .current_dir(directory.path())
        .output()
        .expect("coderadar binary is runnable");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        directory.path().join("report.md").exists(),
        "the default report file is `report.md`"
    );
}

#[test]
fn out_is_rejected_in_scan_mode_and_writes_no_file() {
    let directory = tempfile::TempDir::new().expect("temp dir");
    let target = directory.path().join("out.md");

    let output = run(&[
        "scan",
        FIXTURE,
        "--out",
        target.to_str().expect("path is UTF-8"),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "usage errors never write stdout");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`--out` is only valid for `report`"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("usage"), "stderr: {stderr}");
    assert!(!target.exists(), "scan must not create the report file");
}

#[test]
fn an_out_flag_without_a_value_is_a_usage_error() {
    let output = run(&["report", FIXTURE, "--out"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "usage errors never write stdout");
    assert!(String::from_utf8_lossy(&output.stderr).contains("--out"));
}

#[test]
fn markdown_is_implied_by_the_report_command() {
    let output = run(&["report", FIXTURE, "--markdown"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "usage errors never write stdout");
}
