# AGENTS.md

## Repo has two unrelated halves
- **CodeRadar** (active code): `crates/` + `Cargo.toml` (Rust workspace), `rules/manifest.json`, `scripts/check-architecture.sh`, specs in `coderadar-specs/`, roadmap/gates in `PHASES.md`.
- **Vendored review-skill docs** (top-level `0N-*.md` files and `mattpocock-code-review/`, `awesome-skills-code-review/`, `obra-superpowers/`, `copilot-review-and-refactor/`, `alibaba-open-code-review/`, `spillwave-pr-reviewer/`): third-party reference copies. Do not edit, refactor, or "fix" these unless asked. `README.md` (Arabic) indexes them.

## Commands
- Build: `cargo build --workspace --all-targets`
- Test: `cargo test --workspace --all-targets` — single test: `cargo test -p coderadar-infrastructure --test scan_pipeline`
- Lint: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings` (CI sets `RUSTFLAGS: -D warnings`, so any warning fails)
- Architecture guard: `bash scripts/check-architecture.sh` (needs Git Bash/WSL on Windows; the same rules are re-checked by `crates/coderadar-infrastructure/tests/architecture_boundaries.rs`)
- Security guard (NFR-05): `bash scripts/check-security.sh` (needs Git Bash/WSL on Windows; the same rules are re-checked by `crates/coderadar-infrastructure/tests/security_posture.rs`)
- Performance gate (NFR-01): `cargo test --release -p coderadar-infrastructure --test performance_budget --test memory_budget`
- Desktop perf (recorded, not a gate): `npm run build && npx tauri build --no-bundle`, then `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-desktop.ps1 -ExePath src-tauri\target\release\coderadar-desktop.exe -Runs 7 -IdleMs 5000`. A bare `cargo build --release` on `src-tauri` produces a **development** build that loads `devUrl` (a localhost error page), so never measure from it.
- MSRV gate: `cargo +1.82.0 check --workspace --exclude coderadar-desktop --all-targets`
- CLI: `cargo run -p coderadar-infrastructure --bin coderadar -- scan <path> [--pretty]` (JSON on stdout; exit 0 = ok, 1 = runtime error, 2 = usage error)
- CLI Markdown export (FR-07): `coderadar scan <path> --markdown` prints the report on stdout; `coderadar report <path> [--out <file>]` writes it (default file name `report.md`)
- Phase 8 CLI: `coderadar scan <path> --staged` (Git staged changeset + PR hygiene), `coderadar rules verify <file>`, `coderadar rules install <file>`

## Enforced architecture (compiler + CI, not suggestive)
- Four layers: `coderadar-domain` (std-only, **zero** deps) → `coderadar-usecases` (only `coderadar-domain`; async via `std::future::Future` trait objects in `src/ports.rs`) → `coderadar-adapters` (never `coderadar-infrastructure`) → `coderadar-infrastructure` (only layer allowed external crates; hosts the `coderadar` binary).
- Forbidden in `usecases`: tokio, async-std, smol, futures, reqwest, sqlx, hyper, serde, outer crates. Inner crates must never reference outer crates. Fix violations by moving code outward, not by loosening the guard.
- All four crates enable `#![deny(missing_docs)]` in `lib.rs`; `scripts/check-architecture.sh` greps sources/Toml files directly, so comments mentioning forbidden crate names in code (not manifests) can still trip it.

## MSRV pins — avoid casual `cargo update`
- Workspace `rust-version = "1.82"`. Exact pins exist because newer releases require Rust 1.85+ (edition2024): `ignore =0.4.23`, `globset =0.4.15`, `tempfile =3.14.0` in Cargo.toml; `tree-sitter-language 0.1.5`, `indexmap 2.7.1`, `icu_* 2.2.0`, and `idna_adapter 1.1.0` pinned in `Cargo.lock`. Phase 6 added `sha2 =0.10`, `rusqlite =0.32.1` and `libsqlite3-sys =0.30.1` (SQLite is bundled, no system library) — also pinned for the same reason. Phase 8 added `git2 =0.20` (`default-features = false`) and `ed25519-dalek =2.2.0`; their transitive build deps needed lock pins too: `jobserver 0.1.32` and `base64ct 1.6.0` (newer releases are edition2024). After any dependency/lockfile change run the 1.82 check above. On Windows use `+1.82.0-x86_64-pc-windows-gnu`; plain `+1.82.0` resolves to the MSVC toolchain and fails without `link.exe`.

## Patch safety (Phase 6)
- Every file write **into a workspace** goes through `WorkspaceGuard` (`crates/coderadar-infrastructure/src/workspace_guard.rs`): absolute paths, `..` components and symlink escapes are refused. `patching.rs::atomic_write` is crate-private for that reason — its callers are a closed set: `SafePatchApplier` (Phase 6, workspace-scoped), the Markdown export (`export.rs`, Phase 7, whose target is the path the user picked in the save dialog and is therefore outside any workspace) and the signed rules installer (`rules_ota.rs`, Phase 8, which writes only a manifest that already verified against the embedded public key). The security guard enforces that closed set.
- Every write is atomic: staging file in the same directory + `rename` (`patching.rs::atomic_write`). The staging file is created with `create_new`, so a path planted by someone else is never opened, and the target's permissions are copied onto it (a failed copy aborts the write).
- Every patch carries the SHA-256 of the file it was built against; a mismatch is a `PatchOutcome::Conflict` and nothing is written.
- Edit offsets coming from the UI must fall on UTF-8 character boundaries (`domain::patch::apply_edits`); anything else is a `PatchOutcome::Rejected`, never a panic.
- The scan-history database is opened on a background thread (`src-tauri::commands::HistoryHandle`); a persistence failure is logged and never invalidates a finished scan.

## Rules manifest contract
- `rules/manifest.json` is `include_str!`-embedded at compile time (`rules_manifest.rs`) — edits need a rebuild; keep the file at its exact path.
- The count 19 is asserted in three places: CI `rules-manifest` job (inline python), `crates/coderadar-infrastructure/tests/rules_manifest.rs`, and `RuleManifest::validate()` (`total_rules` == `rules.len()`, no duplicate IDs). Severity must be `LOW|MEDIUM|HIGH|CRITICAL`; `signature` is verified at runtime by `RulesStore::verify` (Ed25519 over canonical JSON). Adding/removing a rule means updating all of these and re-signing `rules/manifest.json` (`scripts/sign-rules.mjs`).

## Phase 8 invariants (PR mode, Tier 2, OTA)
- A staged scan reads only the Git index (`git2`, no process spawn); deleted/hidden/ignored/escaping paths are skipped. `pr_hygiene` is present only for `staged_diff`.
- Tier 2 providers return a parsed draft; `GenerateFix` owns empty/oversized/unchanged rejection and proves the whole-file edit reproduces the payload. Nothing is written by generation: the Phase 6 `apply_patch` is the only writer.
- The cloud provider refuses any file where redaction changes the content, and the Tauri `generate_fix` command refuses `gemini` without `allow_cloud`; the UI has a second consent checkbox. The Gemini key lives in the keychain (`GeminiFixProvider` account) or `GEMINI_API_KEY`.
- `RulesStore` verifies before installing, installs atomically into the platform data dir (`CODERADAR_DATA_DIR` overrides), and falls back to `EMBEDDED_MANIFEST_JSON` on any corruption. The private signing key is not in the repo.

## Testing quirks
- `crates/coderadar-infrastructure/tests/fixtures/known-bad-repo/expected-findings.json` fixes the *exact* expected set (15 findings as `(rule_id, file, line_start)` triples); scan tests assert exact equality, so heuristic or fixture changes require updating it.
- `tests/memory_budget.rs` is **no longer Linux-only**: the peak-RSS sampler in `tests/common/rss.rs` is cross-platform (Linux `VmHWM`, macOS `ps` with the maximum over the samples, Windows `PeakWorkingSet64`), so the 45 MB budget is measured on every platform, not only in CI. The cold-start gate lives in `tests/performance_budget.rs` (1 200 ms in a release build, relaxed to 5 000 ms in debug).
- CLI tests spawn the real binary via `env!("CARGO_BIN_EXE_coderadar")`.
- The release workflow `.github/workflows/release.yml` builds a **draft** release with the `.msi`/`.exe` (Windows), `.dmg` (macOS aarch64 + x86_64) and `.deb`/`.AppImage` (Linux) matrix, plus a `SHA256SUMS.txt`; the draft is published by a human after the clean-machine smoke test.

## Desktop shell (Tauri)
- `src-tauri/tauri.conf.json` must **not** carry a `plugins.dialog` entry: since `tauri-plugin-dialog` 2.8 the plugin has no configuration (its config type became a unit), and the entry makes Tauri panic during initialisation — the desktop app then never starts. The dialog needs no configuration at all.
- The desktop crate is excluded from the MSRV gate (`cargo +1.82.0 check --workspace --exclude coderadar-desktop`); the workspace crates are the ones pinned to 1.82.

## Deliberate design calls (do not "fix")
- The CLI has two export surfaces on purpose: `coderadar scan <path> --markdown` prints to stdout, `coderadar report <path> [--out <file>]` writes a file. `--out` in scan mode is a usage error (exit 2), never a silently ignored flag.
- `tests/common/rss.rs` keeps one loop per OS (`VmHWM` / `ps` / `PeakWorkingSet64`) instead of a unifying abstraction: the branches differ in the data source, not only in the command.
- Scan-history rows are read with a small hand-rolled JSON reader to keep the SQLite layer dependency-light; do not swap it for serde without re-reading the dependency rule.
- `Severity` / `Category` cross the CLI boundary as strings (DTO), not as domain enums — the wire format is the outer layer's convenience.
- Findings precision (`feat/findings-precision`, post-Phase-8): the pipeline drops mock-heuristic findings below `HEURISTIC_CONFIDENCE_FLOOR = 0.5` (inclusive, `CR-SPEC-09 §5`); dropped findings are only **counted** (`suppressed_heuristic_findings`), never logged individually. Tier 0 findings are never filtered and staged-diff PR findings bypass the floor. Every finding carries the engine that produced it (`tier0` | `tier1` | `mock`) — tag it at result construction, never from `rule.engine`, which is the *expected* engine — and every session records `engine_mode` (`mock` | `jev` | `mock_fallback`; the CLI offline compositions hardwire `mock`). The fixture keeps exactly 15 findings; `EDGE-PRM-002` sits exactly at the floor.

## Workflow
- Phase-gated delivery (`PHASES.md`): one branch + one PR per phase task, merge only when CI is green, never skip a gate. Phase 7's engine half is verified in CI; the clean-machine install on each OS is the open human step (the draft release holds the installers). Phase 8 (post-MVP: PR mode + Tier 2 auto-fix + signed OTA rules) is implemented in `feat/phase-8-post-mvp`; its live-provider smoke test is the open human step.
- Code, comments, and commit messages are English; `README.md` is Arabic — keep it Arabic when editing. `coderadar-specs/*.md` are written in English (the Phase 0-5 documents) — follow the language of the document you edit.
- Code comments cite spec sections (`CR-SPEC-02 §3`); before changing architecture or domain rules, read the matching `coderadar-specs/` document.
