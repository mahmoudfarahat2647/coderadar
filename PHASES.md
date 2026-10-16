# CodeRadar Studio — Delivery Phases

> Companion to [`coderadar-specs/01-BRD-AND-MVP-REQUIREMENTS.md`](./coderadar-specs/01-BRD-AND-MVP-REQUIREMENTS.md) and [`coderadar-specs/07-DELIVERY-PHASES.md`](./coderadar-specs/07-DELIVERY-PHASES.md). Each phase ends with a **gate**: don't start the next phase until the gate passes.
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
| 7 | Hardening, packaging, release v1.0 | 4–5 d | Win/macOS/Linux builds pass smoke test |
| 8 | Post-MVP (v1.1) | later | PR mode + Tier 2 auto-fix |

**Total MVP (phases 0–7): about 31–41 focused days.**

---

## Phase 0 — Spike & spec cleanup ✅ (see `coderadar-specs/08-PHASE-0-SPIKE-AND-DECISIONS.md`)
**Goal:** remove the biggest unknown before writing real code.

- [x] Call the Jev API with 10-chunk batches over HTTP/2 from your real network; record latency and payload size *(endpoints probed live; authenticated inference pending a `jv_live_` key — CR-SPEC-08 §6)*
- [x] Decide the latency SLA *(two-tier: Mock 10–35 ms/chunk; live Jev 70–500 ms/request ≈ 16–59 ms/chunk amortized — CR-SPEC-08 §3)*
- [x] Choose Tier 1 languages for v1.0 *(TypeScript/JavaScript — CR-SPEC-08 §4)*
- [x] Fix doc inconsistencies (radar node capacity, Markdown export priority, `PRD.md` link — CR-SPEC-08 §5)
- [x] Verify GitHub milestones `Phase 0` … `Phase 8` *(all 9 already exist — CR-SPEC-08 §7)*

**Gate:** a written benchmark number. ✅ PASS — Jev is **significantly slower than 35 ms** (vendor floor 70 ms/request; measured RTT ~90 ms, TLS ~262 ms). Mock-first stays the default and the PRD claims were corrected.

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

## Phase 7 — Hardening and release v1.0
**Goal:** something others can install.

- [ ] Markdown report export
- [ ] Cold start < 1.2 s and idle RSS < 45 MB measured and recorded
- [ ] Security review against NFR-05 (path traversal, CSP, keychain)
- [ ] GitHub Actions release builds: Windows, macOS, Linux
- [ ] README with screenshots, install steps, and known limits

**Gate:** clean-machine install on each OS; full scan → fix → history works.

## Phase 8 — Post-MVP (v1.1+)
- Git Changeset engine and PR Hygiene scorecard (`PR-*` rules)
- Tier 2 auto-fix via Ollama or a free-tier cloud LLM
- Signed OTA rule updates (Ed25519)
- Then v1.5/v2.0: PR auto-commenter, rule builder, team sync, IDE extensions

---

## Working rules
1. One branch and one PR per phase task; merge only when CI is green.
2. Never skip a gate. Cutting scope is fine, skipping verification is not.
3. If a phase runs over 1.5× its estimate, cut P1 items before extending the date.
