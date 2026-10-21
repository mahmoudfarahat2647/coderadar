# CodeRadar Studio: Phase 7 - Hardening, Packaging & Release v1.0

> **Document ID:** `CR-SPEC-15`
> **Status:** Implemented and verified by the machine; release drafted - the clean-machine install smoke test is the open human step (see §8)
> **Version:** 1.0.0
> **Milestone:** [Phase 7 - Hardening, packaging, release](https://github.com/mahmoudfarahat2647/coderadar/milestone/8)
> **Closes task line:** Phase 7 checklist of [`PHASES.md`](../PHASES.md) and [`07-DELIVERY-PHASES.md`](./07-DELIVERY-PHASES.md)
> **Companion to:** [`14-PHASE-7-SECURITY-REVIEW.md`](./14-PHASE-7-SECURITY-REVIEW.md) (`CR-SPEC-14`, the NFR-05 review of this phase), [`13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md`](./13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md), [`01-BRD-AND-MVP-REQUIREMENTS.md`](./01-BRD-AND-MVP-REQUIREMENTS.md) (`FR-07`, `NFR-01`, `NFR-05`), [`02-CLEAN-ARCHITECTURE-BLUEPRINT.md`](./02-CLEAN-ARCHITECTURE-BLUEPRINT.md)
> **Gate:** *"clean-machine install on each OS; full scan → fix → history works."*

---

## 1. Goal and tasks

Turn the engine into something other people can install: a versioned, signed-off
release with a documented product, measured performance, an enforced security
posture and a packaging pipeline.

The phase had five tasks. Each one is listed below with the artefact that
proves it.

| # | Task | Delivered by | Evidence |
| :--- | :--- | :--- | :--- |
| 1 | **Markdown report export** (`FR-07`) | A pure Layer 3 renderer, an atomic Layer 4 writer, two CLI modes and one desktop command | `crates/coderadar-adapters/src/report_markdown.rs` (6 unit tests), `crates/coderadar-infrastructure/src/export.rs` (5 unit tests), `crates/coderadar-infrastructure/tests/cli.rs` (5 export cases) — see §2 |
| 2 | **Cold start < 1.2 s and idle RSS < 45 MB measured and recorded** (`NFR-01`) | An out-of-process RSS sampler, two gate files, a CI job and a measurement document | `crates/coderadar-infrastructure/tests/performance_budget.rs` (3 gates), `crates/coderadar-infrastructure/tests/memory_budget.rs` (1 gate), `crates/coderadar-infrastructure/tests/common/rss.rs`, `docs/performance-budget.md` — see §3 |
| 3 | **Security review against `NFR-05`** (path traversal, CSP, keychain) | A shell guard, its cross-platform Rust twin, a tightened CSP and a webview without remote resources | `scripts/check-security.sh`, `crates/coderadar-infrastructure/tests/security_posture.rs` (7 invariants), `coderadar-specs/14-PHASE-7-SECURITY-REVIEW.md` (`CR-SPEC-14`) — see §4 |
| 4 | **GitHub Actions release builds: Windows, macOS, Linux** | A four-leg matrix that builds the installers and a checksum job | `.github/workflows/release.yml`, `src-tauri/tauri.conf.json` — see §5 |
| 5 | **README with screenshots, install steps and known limits** | The product README and the documentation of this phase | `docs/README.md`, this document, `coderadar-specs/README.md` — see §6 |

The version of the workspace, of the desktop crate and of the web application
is `1.0.0`. The `tool.version` field of the JSON report follows automatically
(`env!("CARGO_PKG_VERSION")`), so a report always names the build that
produced it.

## 2. The Markdown export (`FR-07`)

`FR-07` requires the current scan to be exportable as a structured Markdown
report (findings, severities, CWE tags, radar metrics, scan metadata).

### 2.1 The renderer / writer split

The export respects the dependency rule like every other feature:

| Layer | Artefact | Responsibility |
| :--- | :--- | :--- |
| 3 — Interface Adapters | `crates/coderadar-adapters/src/report_markdown.rs` | A **pure, deterministic** Markdown presenter. It performs no I/O and never sees an inner entity: the caller hands it a flat `MarkdownReportInput` of `MarkdownFinding` DTOs and gets back a `MarkdownReport`. The same scan always produces byte-identical Markdown. |
| 4 — Frameworks & Drivers | `crates/coderadar-infrastructure/src/export.rs` | Adapts the `ScanReport` to the renderer input and writes the document to disk. |

The rendered document has four parts:

1. **Header** — tool name and version, ruleset version, layer-map provenance
   (`config_file` or `fallback`), session id, root directory, scan mode and the
   UTC start/finish instants.
2. **Executive summary** — the scan metrics (files scanned, AST chunks, module
   chunks, duration, compliance) and the severity distribution.
3. **Radar metrics** — one row per pillar quadrant with the mean and the max
   risk of the findings it holds.
4. **Findings** — grouped by pillar, sorted by severity, each entry carrying
   the path, the line span, the rule id, the severity, the CWE, the title, the
   description, the confidence and the risk score; closed by a one-line
   summary.

Markdown characters in the finding text are escaped, so a title containing a
pipe cannot break the table.

### 2.2 The atomic write

`write_markdown_report` delegates to the **same** `atomic_write` helper the
patch applier uses (`crates/coderadar-infrastructure/src/patching.rs`): the
document is staged in a file next to the target, created with `create_new`,
and then `rename`d over the target. A reader therefore never observes a
half-written report, and a path planted by somebody else is never opened. The
target's permissions are copied onto the staging file; a failed copy aborts
the write. Missing parent directories are created.

The export is the **only** writer of the application that is not contained by
the `WorkspaceGuard`: its target is an explicit path chosen by the user in the
OS save dialog, so containing it in the workspace root would make the feature
useless. The guard therefore enforces a *closed set of callers* of
`atomic_write` (`patching.rs` and `export.rs`) instead of a single caller — see
[`CR-SPEC-14 §6.1`](./14-PHASE-7-SECURITY-REVIEW.md).

### 2.3 The CLI

```
coderadar scan <path> [--pretty] [--markdown]
coderadar report <path> [--out <file>]
```

* `scan` prints the JSON report on stdout; with `--markdown` it prints the
  Markdown report instead.
* `report` runs the scan and writes the Markdown report to a file. Without
  `--out` the target is `report.md` (`DEFAULT_REPORT_FILE_NAME`).
* `--markdown` together with `report` is a usage error, because it is implied.

The exit-code contract is unchanged: `0` ok, `1` runtime failure, `2` usage
error.

### 2.4 The desktop command

The header carries an **Export** button (`src/components/Header.tsx`). It opens
the native save dialog (`dialog:allow-save`, `src/services/tauri.ts:263`) and
then invokes the new Tauri command `export_report`
(`src-tauri/src/commands.rs:517`), which writes the report of the **last
finished scan** through the same atomic writer. The command returns an
`ExportOutcomeDTO` (`path`, `bytes`, `findings`).

### 2.5 Tests

| Requirement | Test |
| :--- | :--- |
| The report carries a summary, the severity distribution and the findings | `crates/coderadar-adapters/src/report_markdown.rs` — `the_report_carries_an_executive_summary_and_the_findings`, `a_clean_scan_says_so`, `the_findings_are_sorted_by_severity` |
| The rendering is deterministic and safe | `the_rendering_is_deterministic`, `markdown_characters_in_the_text_are_escaped`, `timestamps_are_rendered_as_utc_instants` |
| The write is atomic and leaves no staging file | `crates/coderadar-infrastructure/src/export.rs` — `the_report_is_written_atomically`, `a_missing_directory_is_created` |
| The CLI exposes both modes | `crates/coderadar-infrastructure/tests/cli.rs` — `markdown_scan_prints_the_markdown_report`, `the_report_command_writes_a_markdown_file`, `the_report_command_defaults_to_report_md`, `an_out_flag_without_a_value_is_a_usage_error`, `markdown_is_implied_by_the_report_command` |

## 3. The performance audit (`NFR-01`)

### 3.1 Method

The budget has two parts: a cold start below 1 200 ms and an idle RSS of the
Rust core below 45 MB. Both are measured on the **CLI**, because the CLI is
the Rust core in its shipped form: the desktop shell owns the webview, while
the engine, the rule manifest and the scanner live in
`coderadar-infrastructure`. The resident set is sampled **out of process** — an
in-process measurement cannot observe the high-water mark of a file walk,
because the walk streams its chunks to the sink and never holds the whole
project at once.

The sampler (`crates/coderadar-infrastructure/tests/common/rss.rs`) is
cross-platform: the `VmHWM` counter of `/proc/<pid>/status` on Linux, the
current RSS of `ps -o rss=` on macOS (the maximum over the samples is kept),
and `PeakWorkingSet64` on Windows. The three gates of the file are serialised
by a mutex, because a 5 000-file scan running next to a cold-start
measurement inflates the cold start by an order of magnitude.

The complete method, the sample counts and the limits of the measurement are
in [`docs/performance-budget.md`](../docs/performance-budget.md).

### 3.2 Measured numbers

Release build, reference machine: Windows 10 Pro 10.0.19045 x64, Intel Core
i5-10310U @ 1.70 GHz, 16 GB RAM.

| Measurement | Samples | Observed | Budget | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| CLI cold start (process start → report) | 5 | 75–108 ms | < 1 200 ms | **PASS** |
| Desktop cold start (spawn → `RunEvent::Ready`) | 7 | 395–478 ms, median 442 ms | < 1 200 ms | **PASS** |
| Idle peak RSS (Rust core, CLI) | 5 | 4.4–4.5 MB | < 45 MB | **PASS** |
| Idle peak RSS (desktop shell, release) | 1 × 5 s hold | 26.1 MB | < 45 MB | **PASS** |
| Peak RSS of a 5 000-file scan | 3 | 10.3–10.6 MB | < 45 MB | **PASS** |
| Full scan of 5 000 files (engine) | 5 | 1.2–1.6 s | *not budgeted* | — |

The desktop rows are **recorded, not enforced**: the shell is outside the
cargo workspace. A `CODERADAR_BOOTMARK` probe compiled into the shell prints
`bootmark=<ms>` on `tauri::RunEvent::Ready`, and `scripts/measure-desktop.ps1`
samples it plus the idle working set against the bundle produced by
`npx tauri build --no-bundle`. A bare `cargo build --release` builds the
development configuration (it loads `devUrl`, not the bundled assets) and
must not be used for this measurement — see D-7.

### 3.3 The gates

| File | Gate |
| :--- | :--- |
| `crates/coderadar-infrastructure/tests/performance_budget.rs` | `the_cold_start_of_the_rust_core_stays_under_the_budget`, `the_idle_rust_core_stays_under_the_memory_budget`, `a_full_scan_of_a_large_repository_is_measured` |
| `crates/coderadar-infrastructure/tests/memory_budget.rs` | `five_thousand_file_scan_stays_under_the_memory_budget` |

```bash
cargo test --release -p coderadar-infrastructure \
    --test performance_budget --test memory_budget
```

The CI job `performance` (`.github/workflows/ci.yml`) runs exactly this command
in release mode on `ubuntu-latest`. The in-test cold-start budget is relaxed
to 5 000 ms in a debug build; the 1 200 ms budget is only meaningful for the
shipped release profile.

**`tests/memory_budget.rs` is no longer Linux-only.** It used to be
`#![cfg(target_os = "linux")]`, which meant it silently compiled to nothing on
a Windows or macOS developer machine and the budget was only ever measured in
CI. The sampler is now cross-platform (§3.1) and the gate runs everywhere.

## 4. The security review (`NFR-05`)

The review itself is a separate document:
[`14-PHASE-7-SECURITY-REVIEW.md`](./14-PHASE-7-SECURITY-REVIEW.md)
(`CR-SPEC-15`'s companion, `CR-SPEC-14`). It turns the three `NFR-05`
controls into seven enforced invariants:

| # | Invariant | Rust twin |
| :--- | :--- | :--- |
| 1 | The product sources never spawn a process | `the_product_sources_never_spawn_a_process` |
| 2 | Neither the backend nor the front end invokes a shell interpreter | `no_shell_interpreter_is_invoked` |
| 3 | The CSP is strict and free of any remote origin | `the_webview_csp_is_strict_and_free_of_any_remote_origin` |
| 4 | The Tauri ACL grants no dangerous permission | `the_tauri_acl_grants_no_dangerous_permission` |
| 5 | The webview loads no remote resource | `the_webview_loads_no_remote_resource` |
| 6 | The API key is never persisted in plaintext | `the_api_key_is_never_persisted_in_plaintext` |
| 7 | Every write into the workspace goes through the workspace guard | `every_workspace_write_goes_through_the_workspace_guard` |

Two guards assert the same seven invariants: `scripts/check-security.sh` (the
CI job `security`) and `crates/coderadar-infrastructure/tests/security_posture.rs`
(cross-platform, part of `cargo test`).

Two product changes came out of the review:

* **The CSP was tightened.** It is now
  `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data: asset:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; frame-src 'none'; form-action 'none'`
  — no remote origin at all.
* **The remote Google-Fonts resources were removed** from `index.html`, so the
  webview loads no remote asset. The application is now fully offline
  (`NFR-03`); the system font stack replaces Inter / Manrope / JetBrains Mono.

## 5. The release matrix

`.github/workflows/release.yml` is triggered by a `v*` tag or by a manual
dispatch. It resolves the tag, checks it against the application version, and
builds the four legs with `tauri-apps/tauri-action@v0`:

| Platform | Runner | Target | Artefacts |
| :--- | :--- | :--- | :--- |
| macOS | `macos-latest` | `aarch64-apple-darwin` | `.dmg` |
| macOS | `macos-latest` | `x86_64-apple-darwin` | `.dmg` |
| Linux | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `.deb`, `.AppImage` |
| Windows | `windows-latest` | `x86_64-pc-windows-msvc` | `.msi`, `.exe` (NSIS) |

The matrix runs with `fail-fast: false`: one broken platform does not cancel
the other legs. A fourth job computes a `SHA256SUMS.txt` of the installers and
attaches it to the release.

**The release is created as a draft** (`releaseDraft: true`, `draft: true`).
A human installs the artefacts on a clean machine, runs the smoke test of §8,
and only then publishes the release. The same reasoning applies to the
updater: `bundle.createUpdaterArtifacts` is `false` and
`uploadUpdaterJson: false`, so no updater manifest is generated for a build
that ships no updater.

### 5.1 The bundle configuration

`src-tauri/tauri.conf.json` carries the packaging configuration:

| Key | Value |
| :--- | :--- |
| `bundle.active` | `true` |
| `bundle.targets` | `"all"` |
| `bundle.icon` | `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` |
| `bundle.publisher` / `bundle.copyright` | `CodeRadar Studio` |
| `bundle.category` | `DeveloperTool` |
| `bundle.linux.deb.depends` | `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`, `libssl3` |
| `bundle.macOS.minimumSystemVersion` | `10.15` |
| `bundle.windows.nsis.installMode` | `currentUser` |
| `bundle.windows.wix.language` | `en-US` |
| `bundle.createUpdaterArtifacts` | `false` |

## 6. The documentation

| Artefact | Content |
| :--- | :--- |
| `docs/README.md` | The product README: the five pillars and the 19-rule ruleset, the dashboard mockup, the feature tour, the Mermaid diagram of the four layers, the rule tiers and the redaction layer, the install guide (installers, build from source, CLI), the Jev API-key configuration, the performance table, the security posture, the **known limits**, the development commands and the license. |
| `coderadar-specs/15-PHASE-7-HARDENING-PACKAGING-RELEASE.md` | This document. |
| `coderadar-specs/14-PHASE-7-SECURITY-REVIEW.md` | The `NFR-05` review (`CR-SPEC-14`). |
| `coderadar-specs/README.md` | The Arabic index of the specification package, extended with the two new documents. |
| `PHASES.md` and `coderadar-specs/07-DELIVERY-PHASES.md` | The Phase 7 block, in its completed form, with the evidence of each task. |
| `AGENTS.md` | The new commands (security guard, performance gate, Markdown export) and the corrected testing quirks. |
| `docs/performance-budget.md` | The measurement method, the numbers and the limits of the method. |

The root `README.md` is the Arabic index of the vendored third-party review
skills; it is not a product README and was not rewritten. It only points to
`docs/README.md`.

## 7. The defects found and fixed in this phase

| # | Defect | Severity | Fix |
| :--- | :--- | :--- | :--- |
| D-1 | **The desktop application did not start at all.** `tauri-plugin-dialog` resolves to 2.8.3, in which the plugin's configuration type became a unit; the `"plugins": { "dialog": {} }` entry of `src-tauri/tauri.conf.json` could no longer be deserialised and Tauri panicked during initialisation (`PluginInitialization("dialog", "Error deserializing 'plugins.dialog' within your Tauri configuration: invalid type: map, expected unit")`) before any window was created. | **Blocking** | The `plugins.dialog` entry was removed from `src-tauri/tauri.conf.json`. The plugin has had no configuration since 2.8, so the entry was not only invalid but unnecessary. Verified on Windows: the process stays alive and the window is created. (`docs/performance-budget.md` §5 records the defect and the fix.) |
| D-2 | The webview loaded the Inter / Manrope / JetBrains Mono web fonts from `fonts.googleapis.com` and `fonts.gstatic.com`, which broke the offline promise (`NFR-03`) and handed the workspace metadata to a third party. | High | The remote resources were removed from `index.html`; the system font stack is used. Invariant 5 of the security guard prevents a regression. |
| D-3 | The CSP did not forbid `object-src`, `base-uri`, `frame-ancestors`, `frame-src` or `form-action`. | High | The policy was rewritten (§4) and invariant 3 of the security guard prevents a regression. |
| D-4 | `tests/memory_budget.rs` was `#![cfg(target_os = "linux")]`: it silently compiled to nothing on Windows and macOS, so the 45 MB budget was only ever measured in CI. | Medium | The sampler became cross-platform and the gate runs on every platform (§3.3). |
| D-5 | There was no cold-start measurement at all: the `NFR-01` budget of 1 200 ms was a claim, not a number. | Medium | `tests/performance_budget.rs` measures it, the CI job `performance` runs it, and `docs/performance-budget.md` records the numbers and the method. |
| D-6 | `NFR-05` was a *document*, not a *gate*: nothing failed when the CSP was loosened or a `fs:` permission was added to the Tauri ACL. | Medium | `scripts/check-security.sh` and `security_posture.rs` enforce seven invariants (§4). |
| D-7 | The first desktop performance numbers (40–85 ms cold start, 22.3–22.9 MB idle) were measured from a binary built with a bare `cargo build --release`. Tauri builds that way are in their *development* configuration: they load `devUrl` instead of the bundled assets, so the window shows a "can't reach this page" error and the numbers described a shell that never loaded the product. | High | A `CODERADAR_BOOTMARK` probe (inert unless set) prints spawn → `RunEvent::Ready`, `scripts/measure-desktop.ps1` measures the real bundle built by `npx tauri build --no-bundle`, and §3.2 records the fresh numbers plus the reason the old ones were invalid. |
| D-8 | At 125 % display scaling (a ~1536 CSS px viewport) the scan bar overflows: the mode toggle and the *Run Radar Scan* button are clipped by the window edge, because the layout's minimum content width is larger than the viewport. | Medium | Not fixed in this phase — recorded in `docs/README.md` under *Known limits* with the workaround (100 % scaling or a wider window); the responsive fix is Phase 8 work. |

## 8. The verification gate

> **Gate:** clean-machine install on each OS; full scan → fix → history works.

The gate has two halves. The first is verified by the machine; the second
cannot be, from a single Windows development machine. The split is stated
explicitly below rather than glossed over.

### 8.1 Verified in CI and locally

| Part of the gate | How it is verified |
| :--- | :--- |
| **Full scan → fix → history works** | The whole chain is covered by the test suite: `crates/coderadar-infrastructure/tests/scan_pipeline.rs` (scan → findings), `tests/patch_safety.rs` (9 cases: apply, stale hash → conflict, traversal, absolute path, symlink escape, tampered payload, out-of-range offset, missing file, non-character-boundary offset), `tests/patch_flow.rs` (4 end-to-end cases), `tests/scan_history.rs` (9 cases, including *scan → history* end to end and *a failing history does not fail the scan*), `crates/coderadar-domain/tests/patch.rs` (11 cases), `crates/coderadar-usecases/src/fixes.rs` (12 template cases), `crates/coderadar-infrastructure/tests/cli.rs` (the JSON and Markdown report modes), `src/utils/trendGeometry.test.ts` (5 invariant blocks) and `src/utils/radarGeometry.test.ts`. |
| **The installers are produced for the three platforms** | `.github/workflows/release.yml` builds the four legs (Windows `.msi`/`.exe`, macOS `.dmg` ×2, Linux `.deb`/`.AppImage`) and attaches them, with a `SHA256SUMS.txt`, to a draft release. |
| **The desktop application starts** | Verified on Windows after the fix of D-1: the process stays alive and the window is created. The desktop crate is also built and tested in CI (`cargo test (desktop crate)`). |
| **The posture, the budgets and the architecture** | CI jobs `security`, `performance` and `architecture`, plus `cargo fmt --check`, `cargo clippy -- -D warnings` and the MSRV 1.82 check. |

### 8.2 Not verifiable in this environment

| Part of the gate | Why it cannot be verified here | What is done instead |
| :--- | :--- | :--- |
| **Clean-machine install on each target OS** | A clean-machine install can only be executed on the target operating system; this phase was carried out on a single Windows machine, and no macOS or Linux machine was available (and no clean Windows machine either). | The release workflow produces the artefacts for all three operating systems and attaches them to a **draft** release precisely so that a human can run the clean-machine smoke test (install → launch → scan → fix → history) on each OS before the release is published. The draft is the safeguard, not a claim. |
| **The macOS and Linux performance numbers of the shipped build** | No measuring machine was available for those two operating systems. | The CI job `performance` runs the same suite on `ubuntu-latest` in release mode; `docs/performance-budget.md` records the Windows reference numbers and states explicitly that the macOS and Linux numbers of the shipped build are not recorded. |
| **Code signing and notarisation** | Not configured in this phase; it needs certificates and a vendor account. | The installers are unsigned; this is listed as a known limit in `docs/README.md`. |

### 8.3 The gate sentence

> **Gate:** clean-machine install on each OS; full scan → fix → history works. → **PARTIAL** — **(1) Verified by the machine (the engine half):** scan → deterministic fix → history → Markdown export is covered by `tests/patch_safety.rs` (9 cases), `tests/patch_flow.rs` (4), `tests/scan_history.rs` (9), `tests/cli.rs` (5 export cases), `crates/coderadar-domain/tests/patch.rs` (11), `crates/coderadar-usecases/src/fixes.rs` (12) and the trend-geometry invariant blocks, and the `architecture`, `security` and `performance` CI jobs are green; the installers for Windows, macOS (aarch64 + x86_64) and Linux are built by `.github/workflows/release.yml` and attached, with a `SHA256SUMS.txt`, to a **draft** release, and the desktop application starts again after the `tauri-plugin-dialog` 2.8 configuration fix (D-1). **(2) Still open (the human half):** the clean-machine install has not been performed on **any** target OS — macOS and Linux cannot be executed from a single Windows machine, and a Windows clean-machine install was not performed either. **(3) Why a draft release exists:** it holds the installers so that this smoke test can be run before the release is published *(CR-SPEC-15 §8)*.

## 9. Non-goals

* No auto-updater and no signed OTA rule updates (Phase 8).
* No code signing, notarisation or a reproducible-build pipeline (the
  installers are unsigned).
* No LLM-generated (Tier 2) fixes and no Git staged-diff / `PR-*` mode
  (Phase 8).
* No Tier 1 coverage beyond TypeScript and JavaScript.
* No multi-tenant, team-sync or cloud features (v1.5 / v2.0).
* No new runtime dependency was added to the front end or to the desktop
  crate in this phase.
