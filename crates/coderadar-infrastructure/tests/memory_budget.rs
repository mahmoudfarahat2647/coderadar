#![cfg(target_os = "linux")]

//! Linux-only memory budget gate: a 5,000-file repository must scan with a
//! peak RSS (`VmHWM`) below 45 MB.
//!
//! The test spawns the real CLI and samples `/proc/<pid>/status` while it
//! runs, because an in-process measurement could not observe the walk's peak
//! between batches. On non-Linux platforms the crate compiles to nothing.

use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_coderadar");

/// Peak-RSS budget in kB (45 MB).
const BUDGET_KB: u64 = 45 * 1024;
/// Hard timeout for the scan.
const TIMEOUT: Duration = Duration::from_secs(120);
/// Sampling interval for `/proc/<pid>/status`.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(5);
/// Files per subdirectory.
const FILES_PER_DIR: usize = 100;
/// Number of subdirectories.
const DIRS: usize = 50;

fn build_repo(root: &Path) {
    for dir in 0..DIRS {
        let dir_path = root.join(format!("d{dir}"));
        std::fs::create_dir_all(&dir_path).expect("subdirectory is creatable");
        for file in 0..FILES_PER_DIR {
            let index = dir * FILES_PER_DIR + file;
            let source = format!(
                "// synthetic file {index} for the memory-budget gate\n\
                 // padding line two\n\
                 // padding line three\n\
                 export function f{index}(): number {{\n  \
                 const v = {index};\n  \
                 return v;\n\
                 }}\n\
                 // trailing comment one\n\
                 // trailing comment two\n\
                 // trailing comment three\n"
            );
            std::fs::write(dir_path.join(format!("f{index}.ts")), source)
                .expect("synthetic file is writable");
        }
    }
}

/// Read the `VmHWM:` line of `/proc/<pid>/status` in kB, if available.
fn read_vm_hwm_kb(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
    })
}

#[test]
fn five_thousand_file_scan_stays_under_the_memory_budget() {
    let repo = tempfile::TempDir::new().expect("temp dir");
    build_repo(repo.path());

    let mut child = Command::new(BINARY)
        .arg("scan")
        .arg(repo.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("coderadar binary is spawnable");
    let pid = child.id();

    let started = Instant::now();
    let mut peak_kb: u64 = 0;
    let status = loop {
        if let Some(sample) = read_vm_hwm_kb(pid) {
            peak_kb = peak_kb.max(sample);
        }
        if let Some(status) = child.try_wait().expect("try_wait succeeds") {
            break status;
        }
        if started.elapsed() >= TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "scan did not finish within {} s (peak VmHWM so far: {peak_kb} kB)",
                TIMEOUT.as_secs()
            );
        }
        thread::sleep(SAMPLE_INTERVAL);
    };

    assert!(
        status.success(),
        "scan failed with {status} (peak VmHWM: {peak_kb} kB)"
    );
    assert!(
        peak_kb < BUDGET_KB,
        "peak VmHWM was {peak_kb} kB, exceeding the 45 MB budget ({BUDGET_KB} kB)"
    );
}
