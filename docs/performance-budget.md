# Performance budget — NFR-01

Phase 7 of the delivery plan (`PHASES.md` → *Cold start `< 1.2 s` and idle RSS
`< 45 MB` measured and recorded*).

NFR-01 (`coderadar-specs/01-BRD-AND-MVP-REQUIREMENTS.md` §4) budgets:

| Requirement | Text |
| :--- | :--- |
| **NFR-01** | Desktop app cold start in `< 1.2s`. Rust Core Backend RSS `< 45MB` on idle; Webview renderer process isolated and bounded by OS webview runtime. |

This document records **how** both numbers were measured, **what** was
measured on the reference machine, and **where the method stops being
authoritative**.

---

## 1. Method

### 1.1 What is measured

| Budget | Measurable proxy | Harness |
| :--- | :--- | :--- |
| Cold start `< 1.2 s` | The `coderadar` CLI: **process start → JSON report on stdout**, on the curated fixture `tests/fixtures/known-bad-repo` (12 files). | `tests/performance_budget.rs` → `the_cold_start_of_the_rust_core_stays_under_the_budget` |
| Idle RSS `< 45 MB` | The `coderadar` CLI printing its usage line (`--help`): the process **after start-up with no scan**. | `tests/performance_budget.rs` → `the_idle_rust_core_stays_under_the_memory_budget` |
| Peak RSS of a full scan `< 45 MB` | The `coderadar` CLI scanning a synthetic 5,000-file repository (50 directories × 100 files). | `tests/memory_budget.rs` → `five_thousand_file_scan_stays_under_the_memory_budget` |
| Full-scan duration (**not** a budget) | The same 5,000-file repository, reported as the engine's own `scan.duration_ms` and as the wall clock. | `tests/performance_budget.rs` → `a_full_scan_of_a_large_repository_is_measured` |
| **Desktop shell cold start and idle RSS** (recorded, **not** a CI gate) | The `coderadar-desktop` binary produced by the Tauri CLI: `bootmark=<ms>` printed on `tauri::RunEvent::Ready` (spawn → event loop up, window exists), then `PeakWorkingSet64` sampled while the app stays open with no scan. | `scripts/measure-desktop.ps1` (§2.3) |

The CLI is the Rust core in its shipped form: the desktop shell (Tauri) owns
the webview, while the engine, the rule manifest and the scanner live in
`coderadar-infrastructure`. Measuring the CLI therefore measures the code
path the desktop app runs, without the webview in the way.

The desktop numbers are a **recorded measurement, not an enforced gate**: the
shell is not part of the cargo workspace, so no `cargo test` target can build
it. §2.3 states exactly what the probe does and does not cover.

### 1.2 The peak-RSS sampler

The resident set is measured **out of process** — an in-process measurement
cannot observe the high-water mark of a file walk, because the walk streams
its chunks to the sink and never holds the whole project at once. The
sampler lives in `crates/coderadar-infrastructure/tests/common/rss.rs` and
is cross-platform:

| Platform | Source | Semantics | Interval |
| :--- | :--- | :--- | :--- |
| Linux | `VmHWM:` in `/proc/<pid>/status` | the kernel's high-water mark (kB) — the true peak | 5 ms |
| macOS | `ps -o rss= -p <pid>` | the *current* resident set (kB); the **maximum** over the samples is kept | 10 ms |
| Windows | `powershell -NoProfile -NonInteractive -Command "(Get-Process -Id <pid> \| Select-Object -ExpandProperty PeakWorkingSet64)"` | the kernel's peak working set (bytes → kB) | 250 ms |

The Windows probe pays a few hundred milliseconds of PowerShell start-up per
sample, which is why it samples less often than Linux. The PID is a `u32`
produced by the operating system — never user input — and the command is
passed as a fixed argument vector, never through `cmd /C`.

**Short-lived processes.** The CLI that prints `--help` exits within ~6 ms on
Windows, which is faster than any PowerShell round trip, so the idle
measurement uses a dedicated strategy: on Windows the child is started by a
small C# helper (compiled on the fly with `Add-Type`) that polls
`Process.PeakWorkingSet64` at native speed while the child is alive; on Unix
the child is sampled in a tight loop, because reading `/proc` costs
microseconds. The gate fails loudly when no sample could be captured, so a
broken measurement can never be mistaken for a passing one.

### 1.3 Test isolation

The three gates of `tests/performance_budget.rs` are serialized by a
`Mutex`. The test harness runs the tests of one binary in parallel threads,
and a 5,000-file scan running next to a cold-start measurement inflates the
cold start by an order of magnitude (measured: 37 ms alone, up to 838 ms
next to a full scan). The gates therefore run one after the other.

### 1.4 Reference machine

| Item | Value |
| :--- | :--- |
| OS | Windows 10 Pro 10.0.19045 (x64) |
| CPU | Intel Core i5-10310U @ 1.70 GHz |
| RAM | 16 GB |
| Toolchain | `rustc 1.99.0` / `cargo 1.99.0` (stable) |
| WebView2 runtime | 155.0.4283.62 (present) |
| Build profiles | `debug` (`cargo test`) and `release` (`cargo test --release`) |

The machine was shared with other build agents during the measurement; the
gates are serialized but the machine itself is not dedicated. The release
numbers below are therefore the *upper* end of what the same code produces
on an idle machine.

### 1.5 Sample counts

* `performance_budget` (release): 5 runs — cold start, idle RSS, full scan.
* `memory_budget` (release): 3 runs.
* `performance_budget` (debug): 3 runs.
* `memory_budget` (debug): 2 runs.
* Desktop app: 7 runs (`scripts/measure-desktop.ps1`, release build).

---

## 2. Measured numbers

### 2.1 Release build (`cargo test --release`) — the shipped profile

| Measurement | Samples | Observed | Budget | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| **Cold start** (CLI, 12-file fixture, process start → report) | 5 | 75, 79, 85, 95, 108 ms | `< 1,200 ms` | **PASS** (11× margin at the worst sample) |
| **Idle peak RSS** (CLI, `--help`) | 5 | 4,504 – 4,612 kB (4.4 – 4.5 MB) | `< 45 MB` | **PASS** (≈10× margin) |
| **Peak RSS of the 5,000-file scan** | 3 | 10,564 – 10,816 kB (10.3 – 10.6 MB) | `< 45 MB` | **PASS** |
| Full scan of 5,000 files (engine `scan.duration_ms`) | 5 | 1,211 – 1,552 ms | *not budgeted* | — |
| Full scan of 5,000 files (wall clock) | 5 | 1,383 – 2,083 ms | *not budgeted* | — |

### 2.2 Debug build (`cargo test`)

The in-test budget is relaxed to 5,000 ms in debug mode; the real 1,200 ms
budget is only meaningful for the shipped release build.

| Measurement | Samples | Observed | In-test budget | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| Cold start (CLI, 12-file fixture) | 3 | 240, 241, 279 ms | `< 5,000 ms` | **PASS** |
| Idle peak RSS (CLI, `--help`) | 3 | 4,608 – 4,752 kB | `< 45 MB` | **PASS** |
| Peak RSS of the 5,000-file scan | 2 | 13,696 – 13,924 kB | `< 45 MB` | **PASS** |
| Full scan of 5,000 files (engine) | 3 | 4,661 – 5,252 ms | *not budgeted* | — |

### 2.3 Desktop application (recorded, not a CI gate)

Measured with `scripts/measure-desktop.ps1` against the binary produced by
`npx tauri build --no-bundle` (release). The probe is compiled into the shell
and is inert unless `CODERADAR_BOOTMARK` is set; it prints `bootmark=<ms>` on
`tauri::RunEvent::Ready` — the event loop is up and the window exists, which
is *before* the webview paints its first frame.

| Measurement | Samples | Observed | Budget | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| Cold start (`bootmark`, spawn → event loop ready) | 7 | 395, 431, 438, 442, 442, 470, 478 ms — median 442 ms | `< 1,200 ms` | **PASS** |
| Idle peak working set (shell, no scan) | 1 × 5 s hold | 26.1 MB | `< 45 MB` | **PASS** |

The WebView2 renderer runs in its own process (`msedgewebview2.exe`,
≈ 99 MB working set). Per NFR-01 it is *"isolated and bounded by the OS
webview runtime"* and is therefore **outside** the 45 MB budget, which
applies to the Rust core backend only.

> **Correction.** The first cut of this phase recorded 40–85 ms and
> 22.3–22.9 MB here. Those numbers came from a binary built with a bare
> `cargo build --release`, which builds the application in its *development*
> configuration: it loads `devUrl` (`http://localhost:5173`) instead of the
> bundled assets, so the window came up on a "can't reach this page" error and
> the numbers measured a shell that never loaded the product. The probe above
> measures the real bundle; the build command matters and is repeated in §3.

### 2.4 Budget summary

| Budget (NFR-01) | Target | Best observed | Worst observed | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| Cold start | `< 1,200 ms` | 75 ms (CLI, release) | 478 ms (desktop shell, release) | **PASS** |
| Rust core backend RSS, idle | `< 45 MB` | 4.4 MB (CLI, release) | 26.1 MB (desktop shell, release) | **PASS** |

---

## 3. Reproducing the measurement

```bash
# The shipped profile — this is what the CI `performance` job runs.
cargo test --release -p coderadar-infrastructure \
    --test performance_budget --test memory_budget -- --nocapture

# The debug profile (relaxed cold-start budget).
cargo test -p coderadar-infrastructure \
    --test performance_budget --test memory_budget -- --nocapture
```

Expected output of the release run (numbers move with the machine):

```
5,000-file scan: peak RSS 10812 kB, wall clock 2791 ms
full scan: 5000 files, engine 1280 ms, wall clock 1383 ms
cold start: 108 ms (budget 1200 ms)
idle peak RSS: 4604 kB (budget 46080 kB)
```

The desktop app is measured by `scripts/measure-desktop.ps1` (it is not part of
the workspace, so cargo cannot build it for the test). Build the shipped
configuration — **not** `cargo build --release`, which produces a development
build that loads `devUrl` — and run the probe:

```powershell
npm run build
npx tauri build --no-bundle
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-desktop.ps1 `
  -ExePath src-tauri\target\release\coderadar-desktop.exe -Runs 7 -IdleMs 5000
```

---

## 4. Known limits of the method

1. **The CLI is a proxy for the desktop cold start.** The gate measures
   process start → report produced. The desktop app additionally starts a
   Tauri runtime and a WebView2 instance; the webview start-up is dominated
   by the OS webview runtime (process spawn, GPU/renderer initialisation) and
   is **not** covered by this harness. The desktop row in §2.3 measures
   spawn → `RunEvent::Ready` (the event loop is up and the window exists),
   which is still *before* the webview has painted the first frame.
2. **The 45 MB budget applies to the Rust core backend only.** The webview
   renderer is a separate process bounded by the OS webview runtime, as
   stated in NFR-01. Its ≈ 99 MB working set is not a violation of the
   budget and is not measured by the gate.
3. **Warm file cache.** Every number above is a warm-cache launch: the
   executable and the fixture are already in the OS page cache. The first
   launch after a reboot adds the executable's I/O time and is *not*
   represented here.
4. **The idle RSS is the *peak* of an idle process.** The gate reports the
   kernel's high-water mark, not an instantaneous value, so it is an upper
   bound of the idle resident set — the conservative direction for a budget.
5. **Shared machine.** The reference machine ran other builds during the
   measurement (§1.4). The gates are serialized against each other, but not
   against the rest of the machine; the recorded values are the pessimistic
   end of the observed range. The effect is visible on the peak working set of
   the 5 000-file scan on Windows: on an idle machine it measures
   10.5–10.9 MB, while a run that overlapped a release build reached 22.3 MB —
   still less than half of the 45 MB budget, but a reminder that the working
   set of a process depends on how much of the system memory the OS is willing
   to fault in.
6. **macOS is a sampled maximum, not a kernel counter.** `ps` reports the
   current resident set, so a spike between two samples is invisible. The
   sampler therefore keeps the maximum over the samples; the true peak can
   be marginally higher than the recorded value. Linux and Windows expose a
   real high-water mark and are exact.
7. **The Windows idle measurement needs the .NET Framework C# compiler**
   (`Add-Type`), which ships with every supported Windows installation. If
   it is unavailable, the gate fails with a diagnostic instead of silently
   reporting a number.
8. **Only the Windows reference numbers are recorded here.** The CI job
   `performance` runs the same suite on `ubuntu-latest` in release mode; the
   macOS and Linux numbers of the shipped build are not recorded in this
   document, because no macOS machine was available for this measurement.
9. **The desktop probe is not a CI gate.** The shell is outside the cargo
   workspace, so no test target can build it. §2.3 is a recorded measurement
   taken by `scripts/measure-desktop.ps1`; the CI-enforced budgets are the
   Rust-core ones.
10. **A bare `cargo build --release` is not the shipped binary.** Without the
    Tauri CLI the application is compiled in its development configuration
    and loads `devUrl` instead of the bundled assets, so a number measured
    from it says nothing about NFR-01. Build with `npx tauri build
    --no-bundle` (or the full `tauri build`) before measuring.

---

## 5. Blocking defect found while measuring the desktop app

The desktop app **did not start** as committed:

```
thread 'main' panicked at src\main.rs:22:10:
error while running tauri application: PluginInitialization("dialog",
"Error deserializing 'plugins.dialog' within your Tauri configuration:
invalid type: map, expected unit")
```

`src-tauri/Cargo.toml` requires `tauri-plugin-dialog = "2.7.3"`, which
resolves to **2.8.3**; in that release the plugin's configuration became a
unit type, so the `"plugins": { "dialog": {} }` entry of
`src-tauri/tauri.conf.json` can no longer be deserialised. The application
panics during the Tauri initialisation, before any window is created.

**Fixed in Phase 7.** The `plugins` block was removed from
`src-tauri/tauri.conf.json` — the dialog plugin has no configuration — and the
application now starts again (verified on Windows: the process stays alive and
the window is created). The defect and its fix are recorded as `D-1` in
[`coderadar-specs/15-PHASE-7-HARDENING-PACKAGING-RELEASE.md`](../coderadar-specs/15-PHASE-7-HARDENING-PACKAGING-RELEASE.md).

The numbers of §2.3 are measured **after** the fix, from the real release
bundle built by the Tauri CLI, with the `CODERADAR_BOOTMARK` probe and
`scripts/measure-desktop.ps1`. They supersede the earlier scratch-copy
measurement, which was taken from a development build and is not reproducible
from the repository.
