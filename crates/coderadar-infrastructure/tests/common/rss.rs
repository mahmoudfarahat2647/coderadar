//! Cross-platform peak-RSS measurement for the Phase 7 performance gates.
//!
//! NFR-01 budgets the resident set of the Rust core backend at 45 MB while it
//! is idle. The measurement is deliberately taken **out of process**: an
//! in-process measurement could not observe the high-water mark of a file
//! walk, because the walk hands its chunks to the sink as it goes and never
//! holds the whole project at once.
//!
//! # Per-platform strategy
//!
//! * **Linux** — the `VmHWM:` line of `/proc/<pid>/status` (kB). The kernel
//!   keeps the high-water mark of the resident set, so a single read is the
//!   true peak of the process so far.
//! * **macOS** — `ps -o rss= -p <pid>` (kB) reports the *current* resident
//!   set, so the maximum over all samples is kept.
//! * **Windows** — the `PeakWorkingSet64` counter of the process, read with
//!   `powershell -NoProfile -NonInteractive -Command "(Get-Process -Id <pid> |
//!   Select-Object -ExpandProperty PeakWorkingSet64)"` (bytes, converted to
//!   kB). The PID is a `u32` produced by the operating system — never user
//!   input — and the command is passed as a fixed argument vector, never
//!   through `cmd /C`.
//!
//! PowerShell start-up costs a few hundred milliseconds, so Windows samples
//! every 250 ms against 5 ms on Linux.
//!
//! # Short-lived processes
//!
//! The CLI that prints `--help` exits within a few milliseconds — faster than
//! any external probe can attach to it. [`measure_short_lived`] therefore
//! uses a dedicated strategy: on Windows the child is started by a small C#
//! helper (compiled on the fly by `Add-Type`) that polls the kernel counter
//! at native speed while the child is alive, and on Unix the child is
//! sampled in a tight loop, because reading `/proc` costs microseconds.

#![allow(dead_code)]

use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// A child process whose peak resident set was sampled while it ran.
pub struct Sampled {
    /// The highest peak RSS observed, in kB (`0` when no sample was taken).
    pub peak_kb: u64,
    /// The exit status of the child.
    pub status: ExitStatus,
    /// The wall-clock duration of the child, from `spawn()` to its exit.
    pub elapsed: Duration,
}

/// A short-lived child process whose peak resident set was measured.
pub struct ShortLived {
    /// The highest peak RSS observed, in kB (`0` when no sample was taken).
    pub peak_kb: u64,
    /// The exit code of the child, when it could be determined.
    pub exit_code: Option<i32>,
    /// The reason the measurement failed, when it did.
    pub error: Option<String>,
}

/// The interval between two samples of a live process.
///
/// Windows pays a few hundred milliseconds of PowerShell start-up per
/// sample, so it samples far less often than Linux.
pub fn sample_interval() -> Duration {
    if cfg!(target_os = "windows") {
        Duration::from_millis(250)
    } else if cfg!(target_os = "macos") {
        Duration::from_millis(10)
    } else {
        Duration::from_millis(5)
    }
}

/// Sample the peak resident set of a **live** process, in kB.
///
/// Returns `None` when the process has already exited or when the platform
/// offers no way to read the counter.
pub fn peak_rss_kb(pid: u32) -> Option<u64> {
    platform_peak_rss_kb(pid)
}

/// Run `command` to completion while sampling the peak resident set of the
/// child process.
///
/// The child is spawned with its output discarded. The call panics when the
/// child does not finish within `timeout`.
pub fn run_sampled(command: &mut Command, timeout: Duration) -> Sampled {
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the child process is spawnable");
    let pid = child.id();
    let started = Instant::now();
    let mut peak_kb = 0_u64;
    let status = loop {
        if let Some(sample) = peak_rss_kb(pid) {
            peak_kb = peak_kb.max(sample);
        }
        if let Some(status) = child.try_wait().expect("the child status is readable") {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "the child did not finish within {} s (peak RSS so far: {peak_kb} kB)",
                timeout.as_secs()
            );
        }
        thread::sleep(sample_interval());
    };

    Sampled {
        peak_kb,
        status,
        elapsed: started.elapsed(),
    }
}

/// Measure the peak resident set of a process that exits almost immediately.
///
/// Used for the idle measurement: the CLI that prints its usage line and
/// exits is the shortest possible "process after start-up with no scan".
///
/// On Windows the child is started by a C# helper that polls the kernel's
/// peak working-set counter while the child is alive — a PowerShell round
/// trip is slower than the child's whole lifetime. On Unix the child is
/// started here and sampled in a tight loop, because reading the counter
/// costs microseconds.
pub fn measure_short_lived(binary: &str, args: &[&str], timeout: Duration) -> ShortLived {
    #[cfg(target_os = "windows")]
    {
        windows_measure_short_lived(binary, args, timeout)
    }
    #[cfg(not(target_os = "windows"))]
    {
        unix_measure_short_lived(binary, args, timeout)
    }
}

// --- Linux ------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn platform_peak_rss_kb(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
    })
}

// --- macOS ------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn platform_peak_rss_kb(pid: u32) -> Option<u64> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

// --- Windows ----------------------------------------------------------------

#[cfg(target_os = "windows")]
fn platform_peak_rss_kb(pid: u32) -> Option<u64> {
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(format!(
            "(Get-Process -Id {pid} | Select-Object -ExpandProperty PeakWorkingSet64)"
        ))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let bytes: u64 = String::from_utf8(output.stdout).ok()?.trim().parse().ok()?;
    Some(bytes / 1024)
}

// --- Unsupported platforms --------------------------------------------------

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_peak_rss_kb(_pid: u32) -> Option<u64> {
    None
}

// --- Short-lived processes --------------------------------------------------

/// Sample a short-lived child in a tight loop (Unix).
///
/// No sleep between the samples: the child lives for a few milliseconds and
/// reading the counter costs microseconds.
#[cfg(not(target_os = "windows"))]
fn unix_measure_short_lived(binary: &str, args: &[&str], timeout: Duration) -> ShortLived {
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("the child process is spawnable");
    let pid = child.id();
    let started = Instant::now();
    let mut peak_kb = 0_u64;
    let status = loop {
        if let Some(sample) = platform_peak_rss_kb(pid) {
            peak_kb = peak_kb.max(sample);
        }
        if let Some(status) = child.try_wait().expect("the child status is readable") {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "the child did not finish within {} s (peak RSS so far: {peak_kb} kB)",
                timeout.as_secs()
            );
        }
    };

    ShortLived {
        peak_kb,
        exit_code: status.code(),
        error: None,
    }
}

/// The C# helper that measures the peak working set of a short-lived child.
///
/// It polls `Process.PeakWorkingSet64` in a tight loop, which costs a few
/// microseconds per iteration — fast enough to catch a process that only
/// lives a few milliseconds.
#[cfg(target_os = "windows")]
const WINDOWS_PEAK_HELPER: &str = r#"
$ErrorActionPreference = 'Stop'
$source = @'
using System;
using System.Diagnostics;
using System.Threading.Tasks;

public static class CoderadarPeakRss
{
    public static string Measure(string fileName, string arguments)
    {
        var startInfo = new ProcessStartInfo(fileName, arguments);
        startInfo.UseShellExecute = false;
        startInfo.RedirectStandardOutput = true;
        startInfo.RedirectStandardError = true;
        using (var process = Process.Start(startInfo))
        {
            Task<string> stdout = process.StandardOutput.ReadToEndAsync();
            Task<string> stderr = process.StandardError.ReadToEndAsync();
            long peak = 0;
            while (!process.HasExited)
            {
                try
                {
                    process.Refresh();
                    long value = process.PeakWorkingSet64;
                    if (value > peak) { peak = value; }
                }
                catch (Exception)
                {
                    // The child exited between the check and the read.
                }
            }
            process.WaitForExit();
            return "PEAK=" + peak + " EXIT=" + process.ExitCode;
        }
    }
}
'@
Add-Type -TypeDefinition $source -Language CSharp
[Console]::Out.Write([CoderadarPeakRss]::Measure($env:CODERADAR_RSS_FILE, $env:CODERADAR_RSS_ARGS))
"#;

/// Quote one argument for a Windows command line.
///
/// A Windows path can never contain a double quote, so wrapping the argument
/// in double quotes is enough.
#[cfg(target_os = "windows")]
fn quote_windows_argument(argument: &str) -> String {
    if argument.is_empty() {
        return "\"\"".to_string();
    }
    if !argument.contains(' ') && !argument.contains('\t') && !argument.contains('"') {
        return argument.to_string();
    }
    format!("\"{}\"", argument.replace('"', "\\\""))
}

/// Measure a short-lived child with the C# helper (Windows).
#[cfg(target_os = "windows")]
fn windows_measure_short_lived(binary: &str, args: &[&str], timeout: Duration) -> ShortLived {
    let arguments = args
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ");

    let mut command = Command::new("powershell");
    command
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(WINDOWS_PEAK_HELPER)
        .env("CODERADAR_RSS_FILE", binary)
        .env("CODERADAR_RSS_ARGS", arguments);

    let output = match spawn_with_timeout(command, timeout) {
        Some(output) => output,
        None => {
            return ShortLived {
                peak_kb: 0,
                exit_code: None,
                error: Some("the measurement helper did not finish in time".to_string()),
            }
        }
    };
    if !output.status.success() {
        return ShortLived {
            peak_kb: 0,
            exit_code: None,
            error: Some(format!(
                "the measurement helper failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        };
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut peak_kb = 0_u64;
    let mut exit_code = None;
    for field in text.split_whitespace() {
        if let Some(value) = field.strip_prefix("PEAK=") {
            peak_kb = value.parse::<u64>().unwrap_or(0) / 1024;
        } else if let Some(value) = field.strip_prefix("EXIT=") {
            exit_code = value.parse::<i32>().ok();
        }
    }
    ShortLived {
        peak_kb,
        exit_code,
        error: None,
    }
}

/// Run a command and collect its output, killing it after `timeout`.
#[cfg(target_os = "windows")]
fn spawn_with_timeout(mut command: Command, timeout: Duration) -> Option<std::process::Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the helper process is spawnable");
    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .expect("the helper status is readable")
            .is_some()
        {
            return child.wait_with_output().ok();
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
}
