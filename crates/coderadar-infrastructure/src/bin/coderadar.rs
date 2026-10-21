//! `coderadar` CLI — terminal entry point for the CodeRadar engine.
//!
//! ```text
//! coderadar scan <path> [--pretty] [--markdown]
//! coderadar report <path> [--out <file>]
//! ```
//!
//! `scan` prints the [`ScanReport`] as structured JSON on stdout; with
//! `--markdown` it prints the Markdown report instead (`FR-07`). `report` runs
//! the scan and writes the Markdown report to a file.
//!
//! Exit codes: `0` scan completed, `1` runtime failure (bad path, I/O, engine
//! error), `2` usage error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use coderadar_infrastructure::{
    scan_project, write_markdown_report, ScanReport, DEFAULT_REPORT_FILE_NAME,
};

const USAGE: &str = "usage: coderadar scan <path> [--pretty] [--markdown]\n       coderadar report <path> [--out <file>]\n       (`--out` is only valid for `report`)";

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
    let mut pretty = false;
    let mut markdown = false;
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

    let report = match scan_project(&path) {
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
