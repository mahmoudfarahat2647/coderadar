# CodeRadar Studio — Delivery Phases

> Companion to [`01-BRD-AND-MVP-REQUIREMENTS.md`](./01-BRD-AND-MVP-REQUIREMENTS.md) (the BRD/PRD). Each phase ends with a **gate**: don't start the next phase until the gate passes.
> Effort = focused working days for one developer (double it if you work evenings only).

## Overview

| Phase | Name | Effort | Gate (done when…) |
|---|---|---|---|
| 0 | Spike & spec cleanup | 1–2 d | Jev latency measured; doc inconsistencies fixed |
| 1 | Foundation (Clean Architecture skeleton) | 3–4 d | Workspace builds; boundary rules enforced in CI |
| 2 | Scanner + Tier 0 + Mock engine (CLI) | 5–7 d | CLI scans 5,000 files with correct findings, no OOM |
| 3 | Tauri shell + dashboard + findings feed | 5–7 d | App shows live findings, filters work |
| 4 | Polar radar canvas | 4–5 d | 60 FPS with up to 150 nodes; hover/click linked to feed |
| 5 | Jev Tier 1 + privacy layer | 4–5 d | Real Jev results match mock schema; secrets redacted |
| 6 | Diff modal, safe patching, SQLite history | 5–6 d | Patch safety tests pass; history trend visible |
| 7 | Hardening, packaging, release v1.0 | 4–5 d | Engine gate verified in CI; clean-machine install on each OS is the remaining human step (draft release) |
| 8 | Post-MVP (v1.1) | later | PR mode + Tier 2 auto-fix + signed OTA rules |

**Total MVP (phases 0–7): about 31–41 focused days.**

---

## Phase 0 — Spike & spec cleanup ✅ (see `08-PHASE-0-SPIKE-AND-DECISIONS.md`)
**Goal:** remove the biggest unknown before writing real code.

- [x] Call the Jev API with 10-chunk batches over HTTP/2 from your real network; record latency and payload size *(authenticated calls measured: ~0.5–0.6 s/chunk warm, ~$0.00015/chunk — CR-SPEC-08 §3)*
- [x] Decide the latency SLA *(two-tier: Mock 10–35 ms/chunk instant; live Jev ~0.5–0.6 s/chunk background scan — CR-SPEC-08 §5)*
- [x] Choose Tier 1 languages for v1.0 *(TypeScript/JavaScript — CR-SPEC-08 §4)*
- [x] Fix doc inconsistencies (radar node capacity, Markdown export priority, `PRD.md` link — CR-SPEC-08 §5)
- [x] Verify GitHub milestones `Phase 0` … `Phase 8` *(all 9 already exist — CR-SPEC-08 §7)*

**Gate:** a written benchmark number. ✅ PASS — Jev is **~15–20× slower than 35 ms** (measured ~0.5–0.6 s/chunk warm, ~0.9 s cold). Mock-first stays the default and the PRD claims were corrected.

## Phase 1 — Foundation
**Goal:** the architecture rules are enforced by the compiler from day one.

- [ ] Cargo workspace with 4 crates: `domain`, `usecases`, `adapters`, `infrastructure`
- [ ] Domain: `Finding`, `Severity`, `FindingCategory`, `RiskScore` (std only)
- [ ] Use case ports (traits): `SemanticRuleEvaluator`, `ProjectScannerGateway`, `ChunkSink`, `ScanHistoryRepository`
- [ ] `rules/manifest.json` + the 19 rule definitions (ID, title, CWE, severity)
- [ ] CI (GitHub Actions): `cargo build`, `cargo test`, `clippy`, plus a check that `domain` and `usecases` have no forbidden dependencies

**Gate:** CI green; adding `tokio` to `usecases` makes CI fail.

## Phase 2 — Scanner, Tier 0, Mock engine (no UI yet)
**Goal:** a working engine you can run from the terminal.

- [ ] `ParallelFileScanner` using `ignore::WalkParallel` (respects `.gitignore`)
- [ ] Bloat guard: skip files > 1 MB, lines > 1,000 chars, `node_modules`, `vendor`, `target`
- [ ] Chunker: max 300 LOC / 12 KB per chunk (tree-sitter for TS/JS)
- [ ] `.coderadar.json` layer mapping + fallback conventions
- [ ] Tier 0 rules: `ARCH-DEP-001`, `SEC-TOK-002`, `SEC-TLS-005`
- [ ] `MockRuleEvaluator` (regex heuristics) for the remaining rules
- [ ] Simple CLI: `coderadar scan <path>` prints findings as JSON
- [ ] Test fixtures: a small repo with known bugs and expected findings

**Gate:** CLI scans a 5,000-file repo without running out of memory; fixtures produce the expected findings.

## Phase 3 — Tauri shell, dashboard, findings feed
**Goal:** the engine is visible in a desktop window.

- [ ] Tauri 2.0 + React 19 + Tailwind setup; dark theme tokens
- [ ] Native folder picker; Full Scan vs Git Staged Diff toggle
- [ ] Tauri commands + `FindingDTO` + IPC throttler (flush every 100 ms)
- [ ] Metrics ribbon, severity/category filters, findings feed
- [ ] Status bar with Mock Mode badge
- [ ] Strict webview CSP from the start

**Gate:** pick a folder, see findings stream in, filters work, UI stays responsive.

## Phase 4 — Polar radar canvas
**Goal:** the signature feature.

- [ ] Two canvas layers: static nodes + animated sweep (`requestAnimationFrame`, no React state)
- [ ] `RadarPresenter` (polar projection, FNV-1a jitter) + coordinate math tests
- [ ] Severity colors/sizes; clean files draw no nodes
- [ ] Hover feed row ⇄ highlight node; click opens detail
- [ ] HiDPI scaling, `prefers-reduced-motion`
- [ ] Clustering above 150 nodes (can move to P1 if tight)

**Gate:** 60 FPS with up to 150 nodes (clustering activates above 150) on a mid-range laptop (DevTools trace).

## Phase 5 — Jev Tier 1 + privacy layer
**Goal:** swap Mock for real semantic evaluation without touching use cases.

- [ ] `JevRuleEvaluatorAdapter` (`reqwest`, HTTP/2, batches of 10, connection pool)
- [ ] Secret redaction before any outbound payload (+ tests with fake keys)
- [ ] API key stored in OS keychain (`keyring-rs`); auto-fallback to Mock if missing
- [ ] Timeouts, retries, and graceful degradation when offline
- [ ] Benchmark vs the Phase 0 numbers

**Gate:** same app, same UI, real findings; unplugging the network falls back to Mock with the badge shown.

## Phase 6 — Diff modal, safe patching, history
**Goal:** act on findings safely and track progress.

- [ ] Diff modal (unified view), copy patch
- [ ] Apply patch: SHA-256 check, temp file + atomic rename, workspace-root and symlink guard
- [ ] Start with **manual/templated fixes** for a few rules (e.g. `jwt.decode` → `jwt.verify`); LLM fixes come in Phase 8
- [ ] SQLite (`sqlx`) scan history + trend chart on the dashboard

**Gate:** tests prove a changed file is never overwritten; history shows two scans with a trend.

## Phase 7 — Hardening and release v1.0 ✅ (see `coderadar-specs/15-PHASE-7-HARDENING-PACKAGING-RELEASE.md`)
**Goal:** something others can install.

- [x] Markdown report export *(deterministic Layer 3 renderer `crates/coderadar-adapters/src/report_markdown.rs`, atomic Layer 4 writer `crates/coderadar-infrastructure/src/export.rs`, CLI `coderadar scan --markdown` / `coderadar report`, desktop command `export_report` — CR-SPEC-15 §2)*
- [x] Cold start < 1.2 s and idle RSS < 45 MB measured and recorded *(release: 75–108 ms cold start, 4.4–4.5 MB idle RSS, 10.3–10.6 MB peak RSS on a 5 000-file scan — `docs/performance-budget.md`, `tests/performance_budget.rs`, `tests/memory_budget.rs` — CR-SPEC-15 §3)*
- [x] Security review against NFR-05 (path traversal, CSP, keychain) *(seven invariants enforced by `scripts/check-security.sh` + `crates/coderadar-infrastructure/tests/security_posture.rs`; CSP tightened, remote web fonts removed — CR-SPEC-14 §5, CR-SPEC-15 §4)*
- [x] GitHub Actions release builds: Windows, macOS, Linux *(`.github/workflows/release.yml`: `.msi` + `.exe` (NSIS), `.dmg` ×2, `.deb` + `.AppImage`, draft release + `SHA256SUMS.txt` — CR-SPEC-15 §5)*
- [x] README with screenshots, install steps, and known limits *(`docs/README.md`: dashboard mockup, Mermaid layer diagram, install guide, performance table, security posture, known limits — CR-SPEC-15 §6)*

**Gate:** clean-machine install on each OS; full scan → fix → history works. → PARTIAL — **(1) Verified by the machine (the engine half):** scan → deterministic fix → history → Markdown export is covered by `tests/patch_safety.rs` (9 cases), `tests/patch_flow.rs` (4), `tests/scan_history.rs` (9), `tests/cli.rs` (5 export cases), `crates/coderadar-domain/tests/patch.rs` (11), `crates/coderadar-usecases/src/fixes.rs` (12) and the trend-geometry invariant blocks; the `architecture`, `security` and `performance` CI jobs are green; the desktop application starts again after the `tauri-plugin-dialog` 2.8 configuration fix (D-1); and the draft release carries the artefacts produced by `.github/workflows/release.yml` — Windows (`.msi` + `.exe`), macOS (aarch64 + x86_64 `.dmg`), Linux (`.deb` + `.AppImage`) and `SHA256SUMS.txt`. **(2) Still open (the human half):** the clean-machine install on each target OS, which is the human action of this gate and has not been performed on any of them — macOS and Linux cannot be executed from a single Windows machine, and a Windows clean-machine install has not been performed either. **(3) Why a draft release exists:** it holds the installers so that this clean-machine install can be run before the release is published *(CR-SPEC-15 §8)*.

## Phase 8 — Post-MVP (v1.1+) — (see `16-PHASE-8-POST-MVP-PR-HYGIENE-TIER2-OTA.md`)
**Goal:** review a staged pull request, generate fixes the templates cannot produce, and update rules without a new binary.

- [x] Git Changeset engine and PR Hygiene scorecard (`PR-SCP-001`, `PR-BRK-002`, `PR-TST-003`)
- [x] Tier 2 auto-fix via Ollama or a free-tier cloud LLM
- [x] Signed OTA rule updates (Ed25519)
- [ ] Then v1.5/v2.0: PR auto-commenter, rule builder, team sync, IDE extensions

**Gate:** CI green; staged fixture scan emits the expected `PR-*` findings; Tier 2 rejects empty/oversized/unchanged drafts and cloud secrets; tampered manifests are refused and valid ones activate. — PARTIAL — engine/UI implemented and covered by CR-SPEC-16 §7; the live Ollama and Gemini runs remain the human step.

---

## Working rules
1. One branch and one PR per phase task; merge only when CI is green.
2. Never skip a gate. Cutting scope is fine, skipping verification is not.
3. If a phase runs over 1.5× its estimate, cut P1 items before extending the date.
