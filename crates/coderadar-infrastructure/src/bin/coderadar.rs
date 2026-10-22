//! `coderadar` CLI — terminal entry point for the CodeRadar engine.
//!
//! ```text
//! coderadar scan <path> [--staged] [--pretty] [--markdown]
//! coderadar report <path> [--staged] [--out <file>]
//! coderadar rules verify <manifest.json>
//! coderadar rules install <manifest.json>
//! ```
//!
//! `scan` prints the [`ScanReport`] as structured JSON on stdout; with
//! `--markdown` it prints the Markdown report instead (`FR-07`). `report` runs
//! the scan and writes the Markdown report to a file. `--staged` restricts the
//! scan to the staged Git changeset and adds the Pull Request hygiene
//! scorecard. `rules verify|install` validates a signed OTA ruleset.
//!
//! Exit codes: `0` scan completed, `1` runtime failure (bad path, I/O, engine
//! error, invalid signature), `2` usage error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use coderadar_domain::ScanMode;
use coderadar_infrastructure::{
    scan_project_mode, write_markdown_report, RulesStore, ScanReport, DEFAULT_REPORT_FILE_NAME,
};

const USAGE: &str = "usage: coderadar scan <path> [--staged] [--pretty] [--markdown]\n       coderadar report <path> [--staged] [--out <file>]\n       coderadar rules verify <manifest.json>\n       coderadar rules install <manifest.json>\n       (`--out` is only valid for `report`)";

/// The two modes of the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Print the report on stdout.
    Scan,
    /// Write the Markdown report to a file.
    Report,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(code) => code,
    }
}

fn run(args: &[String]) -> Result<ExitCode, ExitCode> {
    if args.first().map(String::as_str) == Some("rules") {
        return run_rules(&args[1..]);
    }

    let mut pretty = false;
    let mut markdown = false;
    let mut staged = false;
    let mut output: Option<PathBuf> = None;
    let mut path: Option<PathBuf> = None;

    let mut iter = args.iter();
    let mode = match iter.next().map(String::as_str) {
        Some("scan") => Mode::Scan,
        Some("report") => Mode::Report,
        Some("--help" | "-h") => {
            println!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
        _ => {
            eprintln!("{USAGE}");
            return Err(ExitCode::from(2));
        }
    };

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--pretty" => pretty = true,
            "--markdown" => markdown = true,
            "--staged" => staged = true,
            "--out" => {
                let Some(value) = iter.next() else {
                    eprintln!("`--out` expects a file path\n{USAGE}");
                    return Err(ExitCode::from(2));
                };
                output = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                eprintln!("unknown flag `{other}`\n{USAGE}");
                return Err(ExitCode::from(2));
            }
            other => {
                if path.is_some() {
                    eprintln!("unexpected extra argument `{other}`\n{USAGE}");
                    return Err(ExitCode::from(2));
                }
                path = Some(PathBuf::from(other));
            }
        }
    }

    let Some(path) = path else {
        eprintln!("missing <path>\n{USAGE}");
        return Err(ExitCode::from(2));
    };

    if mode == Mode::Report && markdown {
        eprintln!("`--markdown` is implied by `report`\n{USAGE}");
        return Err(ExitCode::from(2));
    }

    if mode == Mode::Scan && output.is_some() {
        eprintln!("`--out` is only valid for `report`\n{USAGE}");
        return Err(ExitCode::from(2));
    }

    let scan_mode = if staged {
        ScanMode::StagedDiff
    } else {
        ScanMode::FullScan
    };
    let report = match scan_project_mode(&path, scan_mode) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("error: {error}");
            return Err(ExitCode::FAILURE);
        }
    };

    match mode {
        Mode::Scan => print_scan(&report, pretty, markdown),
        Mode::Report => write_report(&report, output.as_deref()),
    }
}

/// Verify or install a signed OTA ruleset.
fn run_rules(args: &[String]) -> Result<ExitCode, ExitCode> {
    let Some(action) = args.first().map(String::as_str) else {
        eprintln!("missing `rules` action\n{USAGE}");
        return Err(ExitCode::from(2));
    };
    if !matches!(action, "verify" | "install") {
        eprintln!("unknown `rules` action `{action}`\n{USAGE}");
        return Err(ExitCode::from(2));
    }
    let Some(file) = args.get(1) else {
        eprintln!("missing <manifest.json>\n{USAGE}");
        return Err(ExitCode::from(2));
    };
    if let Some(extra) = args.get(2) {
        eprintln!("unexpected extra argument `{extra}`\n{USAGE}");
        return Err(ExitCode::from(2));
    }

    let json = match std::fs::read_to_string(file) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("error: cannot read `{file}`: {error}");
            return Err(ExitCode::FAILURE);
        }
    };

    if action == "verify" {
        return match RulesStore::verify(&json) {
            Ok(manifest) => {
                println!(
                    "signature valid: ruleset {} ({} rules)",
                    manifest.ruleset_version,
                    manifest.len()
                );
                Ok(ExitCode::SUCCESS)
            }
            Err(error) => {
                eprintln!("error: {error}");
                Err(ExitCode::FAILURE)
            }
        };
    }

    let store = RulesStore::new(RulesStore::default_data_dir());
    match store.install(&json) {
        Ok(manifest) => {
            println!(
                "installed ruleset {} ({} rules) in {}",
                manifest.ruleset_version,
                manifest.len(),
                store.manifest_path().display()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            eprintln!("error: {error}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// Print the report of a scan on stdout.
fn print_scan(report: &ScanReport, pretty: bool, markdown: bool) -> Result<ExitCode, ExitCode> {
    if markdown {
        let markdown = coderadar_infrastructure::markdown_from_report(report);
        println!("{markdown}");
        return Ok(ExitCode::SUCCESS);
    }

    let json = if pretty {
        serde_json::to_string_pretty(report)
    } else {
        serde_json::to_string(report)
    };
    match json {
        Ok(json) => {
            println!("{json}");
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            eprintln!("error: failed to serialize report: {error}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// Write the Markdown report of a scan to a file.
fn write_report(report: &ScanReport, output: Option<&Path>) -> Result<ExitCode, ExitCode> {
    let target = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REPORT_FILE_NAME));

    match write_markdown_report(&target, report) {
        Ok(outcome) => {
            println!(
                "report written to {} ({} finding(s), {} bytes)",
                outcome.path, outcome.findings, outcome.bytes
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            eprintln!("error: the report cannot be written: {error}");
            Err(ExitCode::FAILURE)
        }
    }
}
