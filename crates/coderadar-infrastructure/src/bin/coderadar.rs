//! `coderadar` CLI — terminal entry point for the Phase 2 engine.
//!
//! ```text
//! coderadar scan <path> [--pretty]
//! ```
//!
//! Prints the [`ScanReport`] as structured JSON on stdout. Exit codes:
//! `0` scan completed, `1` runtime failure (bad path, I/O, engine error),
//! `2` usage error.

use std::path::PathBuf;
use std::process::ExitCode;

use coderadar_infrastructure::scan_project;

const USAGE: &str = "usage: coderadar scan <path> [--pretty]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(code) => code,
    }
}

fn run(args: &[String]) -> Result<ExitCode, ExitCode> {
    let mut pretty = false;
    let mut path: Option<PathBuf> = None;
    let mut iter = args.iter();
    match iter.next().map(String::as_str) {
        Some("scan") => {}
        Some("--help" | "-h") => {
            println!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
        _ => {
            eprintln!("{USAGE}");
            return Err(ExitCode::from(2));
        }
    }
    for arg in iter {
        match arg.as_str() {
            "--pretty" => pretty = true,
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

    let report = match scan_project(&path) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("error: {error}");
            return Err(ExitCode::FAILURE);
        }
    };

    let json = if pretty {
        serde_json::to_string_pretty(&report)
    } else {
        serde_json::to_string(&report)
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
