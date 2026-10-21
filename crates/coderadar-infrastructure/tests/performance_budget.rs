//! Phase 7 performance gate — the two budgets of NFR-01.
//!
//! * **Cold start `< 1.2 s`** — the Rust core must produce a report within
//!   1.2 s of being started. The gate drives the real `coderadar` binary on
//!   the curated fixture and measures the wall-clock time from `spawn()` to
//!   the exit of the process.
//! * **Idle RSS `< 45 MB`** — the resident set of the Rust core must stay
//!   below 45 MB while it is idle. The gate measures the peak resident set
//!   of a CLI process that only prints its usage line, i.e. the process
//!   after start-up with no scan.
//!
//! The CLI is the measurable proxy of the desktop application: the Tauri
//! shell owns the webview, while the Rust core is this crate. The measured
//! numbers and the limits of the method are recorded in
//! `docs/performance-budget.md`.
//!
//! A debug build is an order of magnitude slower than the shipped release
//! build, so the cold-start budget is relaxed in debug mode; the CI job
//! `performance` runs this suite with `--release` and enforces the real
//! budget.

#[path = "common/rss.rs"]
mod rss;
#[path = "common/synthetic.rs"]
mod synthetic;

use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rss::measure_short_lived;

/// The compiled CLI under test.
const BINARY: &str = env!("CARGO_BIN_EXE_coderadar");
/// Path of the curated fixture repository.
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/known-bad-repo");

/// The cold-start budget of NFR-01.
const COLD_START_BUDGET_MS: u64 = 1_200;
/// A debug build is an order of magnitude slower than the shipped release
/// build, so the in-test budget is relaxed when the test itself is compiled
/// in debug mode (`cargo test --release` enforces the real budget).
const EFFECTIVE_BUDGET_MS: u64 = if cfg!(debug_assertions) {
    5_000
} else {
    COLD_START_BUDGET_MS
};

/// The idle-RSS budget of NFR-01 in kB (45 MB).
const IDLE_RSS_BUDGET_KB: u64 = 45 * 1024;
/// Hard timeout for the idle measurement.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Hard timeout for the large scan.
const SCAN_TIMEOUT: Duration = Duration::from_secs(300);

/// Serializes the three gates.
///
/// The test harness runs the tests of one binary in parallel threads, and a
/// 5,000-file scan running next to a cold-start measurement inflates the
/// cold start by an order of magnitude. The gates therefore run one after
/// the other, on an otherwise idle machine.
static GATE: Mutex<()> = Mutex::new(());

/// Take the gate, ignoring a poisoned lock: a failing gate must not hide
/// the failure of the next one.
fn gate() -> MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The cold start of the Rust core: process start → report on stdout.
#[test]
fn the_cold_start_of_the_rust_core_stays_under_the_budget() {
    let _gate = gate();
    let started = Instant::now();
    let output = Command::new(BINARY)
        .arg("scan")
        .arg(FIXTURE)
        .output()
        .expect("the coderadar binary is runnable");
    let elapsed_ms = started.elapsed().as_millis() as u64;

    assert!(
        output.status.success(),
        "the fixture scan failed with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed_ms < EFFECTIVE_BUDGET_MS,
        "the cold start took {elapsed_ms} ms, exceeding the {EFFECTIVE_BUDGET_MS} ms budget \
         (NFR-01: {COLD_START_BUDGET_MS} ms)"
    );
    println!("cold start: {elapsed_ms} ms (budget {EFFECTIVE_BUDGET_MS} ms)");
}

/// The full-scan number of a 5,000-file repository.
///
/// This is *not* the cold start: the engine has to walk, chunk and evaluate
/// 5,000 files. The duration is reported (the engine's own
/// `scan.duration_ms` and the wall clock) so the two numbers are never
/// confused with each other.
#[test]
fn a_full_scan_of_a_large_repository_is_measured() {
    let _gate = gate();
    let repo = tempfile::TempDir::new().expect("temp dir");
    synthetic::build_repository(repo.path());

    let started = Instant::now();
    let output = Command::new(BINARY)
        .arg("scan")
        .arg(repo.path())
        .output()
        .expect("the coderadar binary is runnable");
    let wall_ms = started.elapsed().as_millis() as u64;

    assert!(
        output.status.success(),
        "the large scan failed with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        wall_ms < SCAN_TIMEOUT.as_millis() as u64,
        "the large scan took {wall_ms} ms, exceeding the {} ms ceiling",
        SCAN_TIMEOUT.as_millis()
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout parses as JSON");
    let engine_ms = report["scan"]["duration_ms"]
        .as_u64()
        .expect("the report carries the scan duration");
    let files = report["scan"]["files_scanned"]
        .as_u64()
        .expect("the report carries the file count");

    println!("full scan: {files} files, engine {engine_ms} ms, wall clock {wall_ms} ms");
}

/// The idle resident set of the Rust core.
///
/// The measured process is the CLI printing its usage line: the process
/// after start-up, with no scan and no work to do.
#[test]
fn the_idle_rust_core_stays_under_the_memory_budget() {
    let _gate = gate();
    let measured = measure_short_lived(BINARY, &["--help"], IDLE_TIMEOUT);

    assert_eq!(
        measured.exit_code,
        Some(0),
        "the idle CLI must exit successfully"
    );
    assert!(
        measured.peak_kb > 0,
        "no peak RSS sample was captured, so the 45 MB budget was never actually measured{}",
        match &measured.error {
            Some(error) => format!(" ({error})"),
            None => String::new(),
        }
    );
    assert!(
        measured.peak_kb < IDLE_RSS_BUDGET_KB,
        "the idle peak RSS was {} kB, exceeding the 45 MB budget ({IDLE_RSS_BUDGET_KB} kB)",
        measured.peak_kb
    );
    println!(
        "idle peak RSS: {} kB (budget {IDLE_RSS_BUDGET_KB} kB)",
        measured.peak_kb
    );
}
