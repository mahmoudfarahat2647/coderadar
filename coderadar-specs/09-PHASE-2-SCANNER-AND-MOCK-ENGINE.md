# 📡 CodeRadar Studio: Phase 2 — Scanner, Tier 0 & Mock Engine (CLI)

> **Document ID:** `CR-SPEC-09`
> **Status:** ✅ Complete — engine green, gate evidence recorded
> **Milestone:** [Phase 2 — Scanner, Tier 0, Mock engine](https://github.com/mahmoudfarahat2647/coderadar/milestone/3)
> **Closes task line:** Issue [#3](https://github.com/mahmoudfarahat2647/coderadar/issues/3)
> **Executed:** 2026-10-17
> **Gate:** *"CLI scans a 5,000-file repo without running out of memory; fixtures produce the expected findings."* → **Gate result: PASS — 5,000 files in 1.56 s at 9.3 MB peak working set (Windows release, local measurement); the Linux `VmHWM < 45 MB` gate test runs in CI; fixture findings match 15/15 exactly.**

---

## 1. TL;DR — the gate numbers

| Check | Result |
| :--- | :--- |
| **5,000-file scan** (Windows, release binary) | **1.56 s**, peak working set **9.34 MB**, report JSON valid: `files_scanned=5000`, `findings=0` (generated clean files) |
| **5,000-file RSS gate** (Linux, CI) | `tests/memory_budget.rs` generates 5,000 files, spawns the CLI and asserts `VmHWM < 45 MB` — executes on ubuntu CI (not runnable on the Windows dev box) |
| **Fixture repo** `tests/fixtures/known-bad-repo` | **15/15 expected findings**, exact match on `(rule_id, file, line_start)` — no extras, no misses; `ignored.ts` (gitignored AWS key) correctly skipped |
| **Single ~0.8 MB file** (release) | **142 ms** (QA pre-fix probe measured 2.24 s on 1 MB — see §5, Defect 1) |
| **Test suite** | **172 tests**, 0 failures (`cargo test --workspace --all-targets`, `RUSTFLAGS=-D warnings`) |
| **CI parity, locally** | `check-architecture.sh` ✅ · `cargo fmt --all -- --check` ✅ · clippy `--all-features -D warnings` ✅ · fresh MSRV 1.82 check ✅ |

Fixture scan anatomy: 12 scannable files (+1 gitignored), 12 FileModule chunks, 12+ AST chunks, 15 findings across 10 rules, `layer_map_source=config_file`, `mode=full_scan`.

---

## 2. What shipped

| Path | Component |
| :--- | :--- |
| `crates/coderadar-infrastructure/src/scanner.rs` | `ParallelFileScanner` — `ignore::WalkParallel`, `.gitignore` aware (`require_git(false)`), prunes `node_modules`/`vendor`/`target`, bloat guard, one `FileModule` chunk per file + AST chunks |
| `crates/coderadar-infrastructure/src/chunker.rs` | tree-sitter TS/TSX/JS chunker — 300 LOC / 12 KB caps, top-level declaration boundaries, full line coverage, error-fallback line windows |
| `crates/coderadar-infrastructure/src/layer_map.rs` | `.coderadar.json` parsing + conventional fallback globs; lexical relative-import resolution (`layer_of_import`) |
| `crates/coderadar-infrastructure/src/tier0/` | `ARCH-DEP-001` (dependency rule), `SEC-TOK-002` (secrets, always masked), `SEC-TLS-005` (plaintext TLS) |
| `crates/coderadar-infrastructure/src/mock.rs` | `MockRuleEvaluator` — 13 regex heuristics, zero-latency by default, `with_latency_ms` for the Phase 3 UI sweep |
| `crates/coderadar-infrastructure/src/router.rs` | `RuleEngineRouter` — splits rule batches by id between Tier 0 and Mock, merges preserving caller order (one result per rule) |
| `crates/coderadar-infrastructure/src/pipeline.rs` | `ScanPipeline` — bounded streaming batches (AST 32 / file-module 4), findings deduped by id + sorted deterministically |
| `crates/coderadar-infrastructure/src/executor.rs` | std-only `block_on` (park/unpark) — no async runtime dependency |
| `crates/coderadar-infrastructure/src/report.rs` | Serializable report DTOs (`ScanReport`, stable CLI JSON contract) |
| `crates/coderadar-infrastructure/src/composition.rs` + `src/bin/coderadar.rs` | Composition root + `coderadar scan <path> [--pretty]` CLI (exit 0 ok / 1 runtime / 2 usage) |
| `crates/coderadar-domain/src/chunk.rs` | `ChunkKind::{AstWindow, FileModule}` + `CodeChunk::file_module` (Phase 2 domain addition) |
| `tests/fixtures/known-bad-repo/` | Curated fixture repo + `expected-findings.json` contract |
| `tests/{scan_pipeline,cli,memory_budget,robustness}.rs` | End-to-end, CLI, Linux RSS gate, 18 adversarial robustness tests |

---

## 3. Rule coverage & engine routing (Phase 2)

| Engine | Rules |
| :--- | :--- |
| **Tier 0 — deterministic** | `ARCH-DEP-001`, `SEC-TOK-002`, `SEC-TLS-005` |
| **Mock — regex heuristics** | `SEC-INJ-001`, `SEC-JWT-003`, `SEC-XSS-004`, `ARCH-SRP-002`, `ARCH-DIP-003`, `ARCH-BND-004`\*, `EDGE-NPE-001`, `EDGE-PRM-002`, `EDGE-BND-003`, `EDGE-TMO-004`, `PERF-NPO-001`, `PERF-HDL-002`\*, `PERF-EVT-003` |
| **Deferred to Phase 8** | `PR-SCP-001`, `PR-BRK-002`, `PR-TST-003` (need the Git changeset engine) |

\* The manifest declares `engine: tier0` for `ARCH-BND-004`/`PERF-HDL-002`; their deterministic Tier 0 implementations are deferred and the mock heuristics cover them meanwhile. The router's ownership set is the three Phase 2 marker rules (per the Phase 2 checklist), and `ScanMode::StagedDiff` returns an explicit "Phase 8" error instead of silently scanning everything.

---

## 4. Design decisions frozen in Phase 2

1. **Chunk kinds drive routing** — AST heuristics only ever see ≤12 KB `AstWindow` chunks; file/module rules (imports, handles, TLS) only see one `FileModule` chunk per file. The pipeline batches each kind separately, so per-batch memory is flat regardless of repository size.
2. **Router splits by rule id, not by manifest engine field** — one result per rule, input order preserved; merging is O(n) and engine tags stay truthful (`Tier0Static` / `MockHeuristic`).
3. **Determinism** — parallel walk order is nondeterministic; findings are deduped by `rule:file:line` id and sorted by `(file, line, rule)`. Three repeated fixture scans produce byte-identical findings (tested).
4. **Secrets are never echoed** — every secret-bearing description is masked (`first 4 chars + …[REDACTED]`); an end-to-end test asserts the raw fixture secrets are absent from the entire serialized JSON document.
5. **Ignore policy** — `.gitignore` honoured even outside a git repository; `node_modules`/`vendor`/`target` always pruned; hidden dot-entries skipped (ripgrep/linter parity — avoids `.next`/`.yarn` generated-bundle noise). Skipping hidden entries is documented in `scanner.rs` and pinned by `hidden_entries_are_skipped_by_design`.
6. **Bloat guard** — >1 MB files, >1,000-char lines, invalid UTF-8 and unsupported extensions are skipped; a file exactly at a boundary is scanned (boundary tests both sides).
7. **Bootstrap pinning** — the dependency tree is pinned to satisfy MSRV 1.82 (`tempfile =3.14.0`, `ignore =0.4.23`, `globset =0.4.15`, lockfile pins for `indexmap`/`tree-sitter-language`); rationale documented in `Cargo.toml`.

---

## 5. QA hardening pass (adversarial review + 18 robustness tests)

**Defect 1 — major, fixed: quadratic scan time on large files.** Mock heuristics re-ran chunk-wide predicates (`content.contains(...)`, chunk-wide regexes) once per line, making a 1 MB file O(n²) in debug: 128 KB → 1.83 s, 256 KB → 5.70 s, 512 KB → 19.93 s, 1 MB → 75.48 s (release: up to 2.24 s). Fix: `scan_chunk_lines_ctx` computes the per-chunk context **once** before iterating lines (`mock.rs`). After the fix: the full robustness suite dropped from 74 s to **0.78 s** and a ~0.8 MB release scan runs in **142 ms**.

**Defect 2 — minor, documented by decision: hidden files are skipped.** `.eslintrc.js` / `.storybook/*.js` are never scanned because the `ignore` crate skips dot-prefixed entries by default. Decision: keep the skip (scanning generated dot-directories such as `.pnp.cjs`/`.yarn` would add false positives), document it in the scanner contract, and pin it with a test. A future release can expose an "include hidden files" toggle; noting here so the recall trade-off is explicit.

**Observations (spec-compliant, pinned by tests).** A bare `fetch(...).catch(...)` still fires `EDGE-TMO-004` (the rule is about missing timeout/abort, not rejection handling). One line carrying two distinct secrets reports the first (consistent with the per-`(rule,file,line)` dedupe contract). A panic-path audit found no `unwrap`/`expect` on user data in non-test code.

---

## 6. Gate evidence log (commands as run)

| Command | Result |
| :--- | :--- |
| `bash scripts/check-architecture.sh` | `[architecture] OK: domain is std-only and usecases depends only on domain.` |
| `cargo fmt --all -- --check` | clean |
| `$env:RUSTFLAGS="-D warnings"; cargo build --workspace --all-targets` | finished, zero warnings |
| `cargo test --workspace --all-targets` | **172 passed, 0 failed** (infra lib 119 · domain 5 · usecases 10 · arch 4 · manifest 7 · scan_pipeline 3 · cli 6 · robustness 18 · memory_budget 0 on Windows) |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo +1.82.0-x86_64-pc-windows-gnu clean -p coderadar-infrastructure; cargo +1.82.0-x86_64-pc-windows-gnu check -p coderadar-infrastructure --all-targets` | fresh check, zero warnings |
| 5,000-file measurement (Windows, `target/release/coderadar.exe`) | `files_scanned=5000` · 1.56 s · peak working set **9.34 MB** · report JSON parsed OK |

---

## 7. Known limits & explicit deferrals

- **Git changeset engine**: `StagedDiff` mode errors out; the three `PR-*` rules return empty results — all Phase 8.
- **`ARCH-BND-004` / `PERF-HDL-002`**: deterministic Tier 0 implementations deferred; mock heuristics cover them now (§3 note).
- **Symlinks** are not followed (walker default) — same class of deliberate skip as hidden entries.
- **Secret recall**: one signature per line is reported; multi-secret lines report the first match.
- **Linux RSS gate** is authoritative in CI; the Windows working-set number above is a local sanity check, not the gate itself.

---

*Phase 2 branch: `feat/phase-2-scanner-tier0-mock` · References: `CR-SPEC-01…04`, `CR-SPEC-07`, `CR-SPEC-08`.*
