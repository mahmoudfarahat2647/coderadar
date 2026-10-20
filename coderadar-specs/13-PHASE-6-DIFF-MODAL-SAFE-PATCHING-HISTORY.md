# CodeRadar Studio: Phase 6 - Diff Modal, Safe Atomic Patching & SQLite History

> **Document ID:** `CR-SPEC-13`
> **Status:** Complete - implemented, gated and verified (see §9)
> **Milestone:** [Phase 6 - Diff modal, safe patching, history](https://github.com/mahmoudfarahat2647/coderadar/milestone/7)
> **Closes task line:** Issue [#7](https://github.com/mahmoudfarahat2647/coderadar/issues/7)
> **Companion:** [`01-BRD-AND-MVP-REQUIREMENTS.md`](./01-BRD-AND-MVP-REQUIREMENTS.md), [`02-CLEAN-ARCHITECTURE-BLUEPRINT.md`](./02-CLEAN-ARCHITECTURE-BLUEPRINT.md)
> **Gate:** *"Tests prove a changed file is never overwritten; history shows two scans with a trend."*

---

## 1. Goal

Turn findings into actions without ever corrupting a user's file, and make the quality of a
codebase observable over time.

Two capabilities ship in this phase:

1. **Safe patching.** A finding can be turned into a concrete, reviewable unified diff.
   The diff is applied only when the file on disk still matches the SHA-256 fingerprint that
   was captured while the diff was generated, and the write itself is atomic
   (temporary file + rename), so a crash or a concurrent edit can never leave a
   half-written file behind.
2. **History.** Every finished scan is snapshotted into a local SQLite database, and the
   dashboard renders the compliance score of every past scan as a trend line.

## 2. Deliverables

| Task (issue #7) | Component | Path |
| :--- | :--- | :--- |
| Patch model (pure, `std` only) | `PatchProposal`, `LineEdit`, `PatchOutcome`, `PatchRejection` | `crates/coderadar-domain/src/patch.rs` |
| Compliance score on a session | `ScanSession::compliance_percent()`, `ScanSessionSummary` extension | `crates/coderadar-domain/src/scan.rs` |
| Ports for the patch boundary | `SourceFileReader`, `PatchApplier` | `crates/coderadar-usecases/src/ports.rs` |
| Deterministic fix templates | `SEC-JWT-003`, `SEC-INJ-001`, `SEC-TOK-002`, `SEC-TLS-005` + `PreparePatch` use case | `crates/coderadar-usecases/src/fixes.rs` |
| SHA-256 fingerprinting | `sha256_hex` | `crates/coderadar-infrastructure/src/hashing.rs` |
| Workspace guard | traversal + symlink escape prevention | `crates/coderadar-infrastructure/src/workspace_guard.rs` |
| Safe applier | hash check, content re-derivation, temp file + atomic rename | `crates/coderadar-infrastructure/src/patching.rs` |
| Source reader adapter | reads a workspace file and fingerprints it | `crates/coderadar-infrastructure/src/source_reader.rs` |
| SQLite history repository | `rusqlite` implementation of `ScanHistoryRepository` | `crates/coderadar-infrastructure/src/history.rs` |
| Unified diff presenter | LCS line diff, unified hunks | `crates/coderadar-adapters/src/diff.rs` |
| Boundary DTOs | `PatchPreviewDTO`, `PatchProposalDTO`, `PatchOutcomeDTO`, `HistoryPointDTO` | `crates/coderadar-adapters/src/dto.rs` |
| Tauri commands | `preview_patch`, `apply_patch`, `get_scan_history` | `src-tauri/src/commands.rs` |
| Diff review modal | unified diff, copy patch, apply, outcome banner | `src/components/DiffModal.tsx` |
| Quality trend chart | SVG trend of the compliance score over scans | `src/components/TrendChart.tsx` |
| Frontend IPC bridge | `previewPatch`, `applyPatch`, `getScanHistory` | `src/services/tauri.ts` |

## 3. Layer map (CR-SPEC-02 dependency rule)

```
coderadar-domain        std only      PatchProposal, LineEdit, PatchOutcome, compliance score
        ^
coderadar-usecases     domain only    SourceFileReader, PatchApplier, fix templates, PreparePatch
        ^
coderadar-adapters     domain+usecases+serde   unified diff presenter, boundary DTOs
        ^
coderadar-infrastructure  external crates    sha2, rusqlite, filesystem, SQLite repository
```

Nothing in `coderadar-domain` or `coderadar-usecases` learns about the filesystem, SQLite, or
Tauri. The `PatchApplier` port is implemented in Layer 4 exactly like `ScanHistoryRepository`,
`ChunkSink` and `ProjectScannerGateway` already are.

## 4. Patch safety model

A patch is applied only when **all** of the following hold. Each one has a dedicated test.

| # | Guard | Failure mode it prevents |
| :--- | :--- | :--- |
| 1 | The relative path is rejected when it is absolute, contains a `..` component, or resolves outside the canonicalised workspace root | Path traversal (`../../etc/passwd`) |
| 2 | The resolved path is canonicalised and must stay under the canonicalised root, so a symlink that points outside the workspace is rejected | Symlink escape |
| 3 | The SHA-256 of the file on disk must equal `original_sha256` captured when the diff was generated | Concurrent modification / stale diff |
| 4 | The edits are re-applied to the *current* file content and must reproduce `new_content` byte for byte | Tampered or mismatched payload |
| 5 | The new content is written to a temporary file in the same directory, flushed, and then `rename`d over the original | Partial writes / corruption on crash |
| 6 | The original file mode is copied onto the temporary file before the rename | Permission loss |

On any failure the original file is left untouched and the caller receives a
`PatchOutcome` that is either `Conflict` (hash mismatch) or `Rejected` (guard failure).

## 5. Fix templates (deterministic, review-before-apply)

Templates are pure text transformations in Layer 2. They never touch the filesystem; they
only produce a line edit that the user reviews in the diff modal before it is applied.

| Rule | Tier | Transformation |
| :--- | :--- | :--- |
| `SEC-JWT-003` | Tier 1 | `jwt.decode(...)` -> `jwt.verify(...)` (the secret argument is supplied from `process.env.JWT_SECRET` when the call only carried the token) |
| `SEC-INJ-001` | Tier 1 | Concatenated / interpolated SQL -> parameterised query (`?` placeholders + parameter array) |
| `SEC-TOK-002` | Tier 0 | Hardcoded credential literal -> `process.env.<SCREAMING_SNAKE_NAME>` |
| `SEC-TLS-005` | Tier 0 | `rejectUnauthorized: false` -> `true`, `TLSv1`/`TLSv1.1` -> `TLSv1.2` |

Every template is *conservative*: when the pattern does not match exactly, no proposal is
produced and the modal explains that no deterministic fix exists for that finding. LLM based
fixes remain Phase 8.

## 6. SQLite history

Table `scan_sessions`:

| Column | Type | Meaning |
| :--- | :--- | :--- |
| `id` | TEXT PRIMARY KEY | session id |
| `root_dir` | TEXT | scanned workspace |
| `mode` | TEXT | `full_scan` / `staged_diff` |
| `started_at_unix_ms` | INTEGER | start timestamp |
| `finished_at_unix_ms` | INTEGER | finish timestamp |
| `files_scanned` | INTEGER | files that passed the bloat guard |
| `total_findings` | INTEGER | finding count |
| `critical` / `high` / `medium` / `low` | INTEGER | severity breakdown |
| `compliance_percent` | INTEGER | 0-100, files without findings / files scanned |
| `findings_json` | TEXT | JSON snapshot of the findings |

The repository is opened on a background thread (`spawn_blocking`) so the UI thread is never
blocked by SQLite, and every write is a single transaction.

## 7. IPC contract

Commands (Tauri, `src-tauri/src/commands.rs`):

| Command | Arguments | Result |
| :--- | :--- | :--- |
| `preview_patch` | `root_path`, `file_path`, `rule_id` | `PatchPreviewDTO` |
| `apply_patch` | `root_path`, `proposal` | `PatchOutcomeDTO` |
| `get_scan_history` | `limit` | `Vec<HistoryPointDTO>` |

DTO shapes (snake_case field names, matching the existing `FindingDTO` convention):

```jsonc
// PatchPreviewDTO
{
  "file_path": "src/bad/jwt.ts",
  "rule_id": "SEC-JWT-003",
  "title": "Unvalidated JWT Header / Alg None",
  "explanation": "jwt.decode() does not verify the signature ...",
  "original_sha256": "9f2c...",
  "new_sha256": "41ab...",
  "original_line_count": 5,
  "unified_diff": "--- a/src/bad/jwt.ts\n+++ b/src/bad/jwt.ts\n@@ -1,3 +1,3 @@\n...",
  "added_lines": 1,
  "removed_lines": 1,
  "warnings": [],
  "proposal": {
    "file_path": "src/bad/jwt.ts",
    "rule_id": "SEC-JWT-003",
    "original_sha256": "9f2c...",
    "original_line_count": 5,
    "edits": [
      { "start_line": 2, "end_line": 2, "start_offset": 34, "end_offset": 52,
        "replacement": "jwt.verify(token, process.env.JWT_SECRET)" }
    ],
    "new_content": "...",
    "unified_diff": "...",
    "explanation": "jwt.decode() reads the payload without checking the signature ..."
  }
}

// PatchOutcomeDTO
{ "status": "applied" | "conflict" | "rejected", "file_path": "...", "message": "...",
  "original_sha256": "...", "new_sha256": null | "..." }

// HistoryPointDTO
{ "id": "...", "root_dir": "...", "mode": "full_scan", "finished_at_unix_ms": 1760000000000,
  "files_scanned": 12, "total_findings": 15, "critical": 3, "high": 4, "medium": 6, "low": 2,
  "compliance_percent": 58 }
```

## 8. UI

* **Diff modal** (`src/components/DiffModal.tsx`) follows the `ApiKeyDialog` pattern: fixed
  backdrop, elevated panel, `stopPropagation` on the panel, ESC / backdrop close. It renders
  the unified diff with `-` lines in the critical colour and `+` lines in the clean colour,
  shows the SHA-256 fingerprint, and offers *Copy Patch* and *Apply Patch*.
* **Trend chart** (`src/components/TrendChart.tsx`) is a hand drawn SVG line chart (no chart
  library, matching the "no new runtime dependency" rule of the project). The geometry is a
  pure function in `src/utils/trendGeometry.ts`, unit tested with the same `tsx` + `node:assert`
  pattern as `radarGeometry.test.ts`.
* The trend is reachable from a new *Trend* tab and is refreshed after every finished scan.

## 9. Verification gate

| Requirement | Test |
| :--- | :--- |
| A modified file is never corrupted or overwritten unsafely | `crates/coderadar-infrastructure/tests/patch_safety.rs` (8 cases) and `crates/coderadar-infrastructure/tests/patch_flow.rs` (4 end-to-end cases) |
| History shows two or more scans with a trend | `crates/coderadar-infrastructure/tests/scan_history.rs` (6 cases) + `src/utils/trendGeometry.test.ts` (5 assertions) |
| Templates are deterministic and conservative | `crates/coderadar-usecases/src/fixes.rs` (10 unit tests) |
| The patch model is pure | `crates/coderadar-domain/tests/patch.rs` (8 tests) |
| The domain stays free of I/O | `crates/coderadar-infrastructure/tests/architecture_boundaries.rs` + `scripts/check-architecture.sh` |

The same gates run in CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` and
the MSRV 1.82 check.

### 9.1 Guard matrix

| Attempted patch | Result |
| :--- | :--- |
| The file still matches the previewed fingerprint | `applied` (atomic write) |
| The file changed after the preview | `conflict` - nothing is written |
| The payload does not match the current content | `rejected` - nothing is written |
| `../outside.ts` | `rejected` - the workspace guard refuses it |
| An absolute path | `rejected` |
| A symlink inside the workspace pointing outside | `rejected` |

## 10. Non-goals

* No LLM generated fixes (Phase 8).
* No Git changeset scanning (Phase 8).
* No Markdown export (Phase 7).
* No signed rule updates (Phase 8).
