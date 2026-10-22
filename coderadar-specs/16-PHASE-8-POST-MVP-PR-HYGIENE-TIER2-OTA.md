# CR-SPEC-16 — Phase 8 (v1.1): PR Hygiene, Tier 2 Auto-Fix, Signed OTA Rules

> Companion to [`07-DELIVERY-PHASES.md`](./07-DELIVERY-PHASES.md) and
> [`13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md`](./13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md).
> Phase 8 is post-MVP: it must not weaken any Phase 1–7 invariant.

## 1. Scope

Three independent deliverables, each usable on its own:

1. **PR mode** — scan only the Git *staged* changes and score the pull request
   hygiene (`PR-SCP-001`, `PR-BRK-002`, `PR-TST-003`).
2. **Tier 2 auto-fix** — when no deterministic template exists, a local Ollama
   model or a cloud Gemini model can propose a whole-file fix, returned as a
   normal diff preview and applied through the Phase 6 safe-patch path.
3. **Signed OTA rules** — a ruleset manifest can be verified (Ed25519) and
   installed outside any workspace; a corrupt or unsigned manifest never
   replaces the embedded one.

Out of scope: PR auto-commenting, rule builder, team sync, IDE extensions.

## 2. Layering

The dependency rule is unchanged and still checked by
`scripts/check-architecture.sh`:

- `coderadar-domain` — `changeset.rs` (`Changeset`, `FileChange`,
  `PullRequestHygiene`, `PrViolation`, thresholds, `assess()`), std only.
- `coderadar-usecases` — `pull_request.rs` (`AssessPullRequest`), `tier2.rs`
  (`FixFinding`, `FixGenerationRequest`, `GeneratedFixDraft`, `GenerateFix`),
  ports `ChangesetGateway` and `GenerativeFixProvider` in `ports.rs`.
- `coderadar-infrastructure` — `git_changeset.rs` (git2), `tier2.rs`
  (reqwest providers), `rules_ota.rs` (signature + install), composition,
  report DTOs, CLI.
- `src-tauri` — commands, keychain, UI bridge. It may depend on
  infrastructure; nothing inner depends on it.

`GenerativeFixProvider::propose_fix` returns an **already parsed**
`GeneratedFixDraft`: the provider owns the wire protocol and the prompt/answer
parsing; the use case owns the safety validation and the proposal shape.

## 3. PR mode (Git staged changeset)

`GitChangesetReader::read_staged(root)` compares the index against `HEAD`
(or the empty tree in a repository without commits) and yields:

- slash-separated paths relative to the repository root;
- additions/deletions and total changed lines;
- removed public API signatures (conservative line scan for `export`/`public`
  declarations that disappear).

`AssessPullRequest` turns a `Changeset` into:

| Rule | Fires when |
|---|---|
| `PR-SCP-001` | more than 400 changed lines **and** more than one top-level area touched |
| `PR-BRK-002` | a removed public signature is detected |
| `PR-TST-003` | production code changed and no test file changed |

Only rules present in the active manifest are emitted, so a ruleset can disable
a PR rule without a code change. Findings are ordinary findings
(`category = "pull_requests"`); the scan report additionally carries
`pr_hygiene` (omitted for full scans) with the metrics used by the UI
scorecard.

`staged_diff` scans only staged text files: deleted, hidden, ignored, missing
and workspace-escaping paths are skipped; the scanner still canonicalises the
root and refuses a directory outside the workspace. The UI enables the
previously deferred toggle; the CLI exposes `--staged`.

## 4. Tier 2 auto-fix

### 4.1 Contract

`GenerateFix::generate(root, finding)`:

1. reads the file through `SourceFileReader` (workspace guard applies);
2. rejects an empty file (`MAX_INPUT_BYTES = 256 KiB`);
3. asks the provider for a draft (`<name>`, `<explanation>`, `<file>` tags,
   fenced block fallback);
4. rejects an empty/oversized (`MAX_OUTPUT_BYTES = 512 KiB`) payload;
5. rejects a payload identical to the current file;
6. builds a whole-file `LineEdit` and proves `apply_edits` reproduces the
   payload byte for byte;
7. warns when the generated file drops the trailing newline.

Nothing is written here. The returned `PatchPreviewDTO` has the same
fingerprint/diff shape as a deterministic preview, so the Phase 6
`apply_patch` command is the only writer and keeps all its safety checks.

### 4.2 Providers

| Provider | Endpoint | Default model | Secret policy |
|---|---|---|---|
| Ollama (local) | `http://127.0.0.1:11434/api/generate` | `qwen2.5-coder:7b` | stays on the machine |
| Gemini (cloud) | `https://generativelanguage.googleapis.com/v1beta` | `gemini-2.5-flash` | **refuses** any file where `redact_source(content) != content` |

Overrides: `CODERADAR_OLLAMA_ENDPOINT`, `CODERADAR_OLLAMA_MODEL`,
`CODERADAR_GEMINI_MODEL`. Generation timeout: 120 s. The prompt delimits the
file in `<source_file>` and states that the file is untrusted data.

Cloud requests require explicit consent: the Tauri command
`generate_fix` rejects `provider = "gemini"` unless `allow_cloud = true`; the
UI additionally shows a consent checkbox. The Gemini key is stored in the OS
keychain (`KeyringCredentialStore::gemini`, account `gemini`) or read from
`GEMINI_API_KEY`; it is never written to a config file or the webview.

## 5. Signed OTA rules

`RulesStore` (infrastructure) owns the only writes outside a workspace that are
not the user-chosen Markdown export; both security guards sanction exactly
`rules_ota.rs` for that reason.

- **Canonical payload**: recursively sorted JSON keys, compact separators, the
  `signature` field removed.
- **Signature format**: `ed25519:<128 hex chars>` over the canonical payload.
- **Verification**: `RulesStore::verify(json)` parses, checks the signature with
  the embedded public key, then validates the manifest
  (`RuleManifest::validate`: count == declared count, unique ids, severity
  tokens).
- **Install**: `install(json)` verifies first, then uses the Phase 6 atomic
  write into `<data_dir>/rules/active-manifest.json` (same directory staging
  file + rename, `create_new` staging).
- **Activation**: `active_or_embedded()` returns the installed manifest only
  when it verifies; otherwise the embedded `EMBEDDED_MANIFEST_JSON`.
- **Data directory**: platform app-data directory; `CODERADAR_DATA_DIR`
  overrides it (tests always use a temp directory).
- **CLI**: `coderadar rules verify <file>` and
  `coderadar rules install <file>`.

The private signing key is **not** in the repository. The embedded public key is
`1f4d2c5f186388bda1d4d53ac04a3731f9cc02b6894da833876b1f03bd3e80e3`. The
current ruleset is `2026.8.0-pr` and still contains exactly 19 rules.

## 6. Report and UI surfaces

- `ScanReport.pr_hygiene: Option<PrHygieneReport>` (`skip_serializing_if`).
- `PrHygieneReport` fields: `base`, `files_changed`, `additions`,
  `deletions`, `lines_changed`, `production_files_changed`,
  `test_files_changed`, `areas_touched`, `scope_creep`, `breaking_change`,
  `missing_tests`.
- UI: the PRs tab renders the scorecard (`PrHygienePanel.tsx`); the scan bar
  enables staged mode; the diff modal offers a Tier 2 generation panel when
  `proposal == null`; Settings stores/removes the Gemini key and shows the
  provider/model state.

## 7. Verification

| Area | Evidence |
|---|---|
| Changeset model | `crates/coderadar-domain/tests/changeset.rs` (12) |
| PR use case | `crates/coderadar-usecases/src/pull_request.rs` (4) |
| Tier 2 use case | `crates/coderadar-usecases/src/tier2.rs` (5) |
| Git reader | `crates/coderadar-infrastructure/src/git_changeset.rs` (6) |
| Staged scanner | `crates/coderadar-infrastructure/src/scanner.rs` (7) |
| OTA | `crates/coderadar-infrastructure/src/rules_ota.rs` (7, incl. embedded manifest verification and tamper rejection) |
| Providers | `crates/coderadar-infrastructure/src/tier2.rs` (6, incl. cloud secret refusal) |
| Guards | `tests/architecture_boundaries.rs`, `tests/security_posture.rs`, both shell guards |
| Frontend | `npm run build` (tsc + Vite) |

## 8. Gate

Phase 8 is complete when:

1. `cargo fmt --check`, clippy with `-D warnings`, the full test suite, the
   architecture guard, the security guard, the MSRV 1.82 check and the
   frontend build are green;
2. a staged scan of a fixture repository yields the expected `PR-*` findings
   and a `pr_hygiene` block;
3. a Tier 2 draft is refused when it is empty/oversized/unchanged and when a
   cloud provider sees an unredactable secret;
4. an edited manifest fails verification and a valid one installs and becomes
   active across a restart.

The human step (not automatable here): run the desktop app once with a local
Ollama model and once with a free Gemini key, then review the generated diff
before applying it.
