//! Memory budget gate: a 5,000-file repository must scan with a peak
//! resident set below 45 MB (NFR-01: "Rust Core Backend RSS `< 45MB` on
//! idle").
//!
//! The test spawns the real CLI and samples the peak RSS of the child
//! process while it runs, because an in-process measurement could not
//! observe the walk's peak between batches.
//!
//! The sampler is cross-platform — see `tests/common/rss.rs` for the
//! per-platform strategy:
//!
//! * **Linux** — the `VmHWM:` high-water mark of `/proc/<pid>/status`.
//! * **macOS** — the resident set reported by `ps -o rss= -p <pid>`, the
//!   maximum over all samples being kept.
//! * **Windows** — the `PeakWorkingSet64` counter of the process, read
//!   through `powershell` (bytes, converted to kB). PowerShell start-up is
//!   expensive, so Windows samples every 250 ms against 5 ms on Linux.

#[path = "common/rss.rs"]
mod rss;
#[path = "common/synthetic.rs"]
mod synthetic;

use std::process::Command;
use std::time::Duration;

use rss::run_sampled;

const BINARY: &str = env!("CARGO_BIN_EXE_coderadar");

/// Peak-RSS budget in kB (45 MB).
const BUDGET_KB: u64 = 45 * 1024;
/// Hard timeout for the scan.
const TIMEOUT: Duration = Duration::from_secs(120);

#[test]
fn five_thousand_file_scan_stays_under_the_memory_budget() {
    let repo = tempfile::TempDir::new().expect("temp dir");
    synthetic::build_repository(repo.path());

    let mut command = Command::new(BINARY);
    command.arg("scan").arg(repo.path());
    let sampled = run_sampled(&mut command, TIMEOUT);

    assert!(
        sampled.status.success(),
        "scan failed with {} (peak RSS: {} kB)",
        sampled.status,
        sampled.peak_kb
    );
    assert!(
        sampled.peak_kb > 0,
        "no peak RSS sample was captured before the process exited, so the 45 MB budget was never actually measured"
    );
    assert!(
        sampled.peak_kb < BUDGET_KB,
        "the peak RSS was {} kB, exceeding the 45 MB budget ({BUDGET_KB} kB)",
        sampled.peak_kb
    );
    println!(
        "5,000-file scan: peak RSS {} kB, wall clock {} ms",
        sampled.peak_kb,
        sampled.elapsed.as_millis()
    );
}
