# AGENTS.md

## Repo has two unrelated halves
- **CodeRadar** (active code): `crates/` + `Cargo.toml` (Rust workspace), `rules/manifest.json`, `scripts/check-architecture.sh`, specs in `coderadar-specs/`, roadmap/gates in `PHASES.md`.
- **Vendored review-skill docs** (top-level `0N-*.md` files and `mattpocock-code-review/`, `awesome-skills-code-review/`, `obra-superpowers/`, `copilot-review-and-refactor/`, `alibaba-open-code-review/`, `spillwave-pr-reviewer/`): third-party reference copies. Do not edit, refactor, or "fix" these unless asked. `README.md` (Arabic) indexes them.

## Commands
- Build: `cargo build --workspace --all-targets`
- Test: `cargo test --workspace --all-targets` — single test: `cargo test -p coderadar-infrastructure --test scan_pipeline`
- Lint: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings` (CI sets `RUSTFLAGS: -D warnings`, so any warning fails)
- Architecture guard: `bash scripts/check-architecture.sh` (needs Git Bash/WSL on Windows; the same rules are re-checked by `crates/coderadar-infrastructure/tests/architecture_boundaries.rs`)
- MSRV gate: `cargo +1.82.0 check --workspace --exclude coderadar-desktop --all-targets`
- CLI: `cargo run -p coderadar-infrastructure --bin coderadar -- scan <path> [--pretty]` (JSON on stdout; exit 0 = ok, 1 = runtime error, 2 = usage error)

## Enforced architecture (compiler + CI, not suggestive)
- Four layers: `coderadar-domain` (std-only, **zero** deps) → `coderadar-usecases` (only `coderadar-domain`; async via `std::future::Future` trait objects in `src/ports.rs`) → `coderadar-adapters` (never `coderadar-infrastructure`) → `coderadar-infrastructure` (only layer allowed external crates; hosts the `coderadar` binary).
- Forbidden in `usecases`: tokio, async-std, smol, futures, reqwest, sqlx, hyper, serde, outer crates. Inner crates must never reference outer crates. Fix violations by moving code outward, not by loosening the guard.
- All four crates enable `#![deny(missing_docs)]` in `lib.rs`; `scripts/check-architecture.sh` greps sources/Toml files directly, so comments mentioning forbidden crate names in code (not manifests) can still trip it.

## MSRV pins — avoid casual `cargo update`
- Workspace `rust-version = "1.82"`. Exact pins exist because newer releases require Rust 1.85+ (edition2024): `ignore =0.4.23`, `globset =0.4.15`, `tempfile =3.14.0` in Cargo.toml; `tree-sitter-language 0.1.5`, `indexmap 2.7.1`, `icu_* 2.2.0`, and `idna_adapter 1.1.0` pinned in `Cargo.lock`. Phase 6 added `sha2 =0.10`, `rusqlite =0.32.1` and `libsqlite3-sys =0.30.1` (SQLite is bundled, no system library) — also pinned for the same reason. After any dependency/lockfile change run the 1.82 check above.

## Patch safety (Phase 6)
- Every file write goes through `WorkspaceGuard` (`crates/coderadar-infrastructure/src/workspace_guard.rs`): absolute paths, `..` components and symlink escapes are refused.
- Every write is atomic: staging file in the same directory + `rename` (`patching.rs::atomic_write`).
- Every patch carries the SHA-256 of the file it was built against; a mismatch is a `PatchOutcome::Conflict` and nothing is written.

## Rules manifest contract
- `rules/manifest.json` is `include_str!`-embedded at compile time (`rules_manifest.rs`) — edits need a rebuild; keep the file at its exact path.
- The count 19 is asserted in three places: CI `rules-manifest` job (inline python), `crates/coderadar-infrastructure/tests/rules_manifest.rs`, and `RuleManifest::validate()` (`total_rules` == `rules.len()`, no duplicate IDs). Severity must be `LOW|MEDIUM|HIGH|CRITICAL`; `signature` is a Phase 8 placeholder (not verified). Adding/removing a rule means updating all of these.

## Testing quirks
- `crates/coderadar-infrastructure/tests/fixtures/known-bad-repo/expected-findings.json` fixes the *exact* expected set (15 findings as `(rule_id, file, line_start)` triples); scan tests assert exact equality, so heuristic or fixture changes require updating it.
- `tests/memory_budget.rs` is `#![cfg(target_os = "linux")]`: it silently compiles to nothing on Windows; the 45 MB RSS budget is only measured in CI.
- CLI tests spawn the real binary via `env!("CARGO_BIN_EXE_coderadar")`.

## Workflow
- Phase-gated delivery (`PHASES.md`): one branch + one PR per phase task, merge only when CI is green, never skip a gate. Phases 0–6 are complete; Phase 7 (hardening, packaging, release v1.0) is next.
- Code, comments, and commit messages are English; `README.md` is Arabic — keep it Arabic when editing. `coderadar-specs/*.md` are written in English (the Phase 0-5 documents) — follow the language of the document you edit.
- Code comments cite spec sections (`CR-SPEC-02 §3`); before changing architecture or domain rules, read the matching `coderadar-specs/` document.
