# CodeRadar Studio: Phase 7 - Desktop AppSec Review (NFR-05)

> **Document ID:** `CR-SPEC-14`
> **Status:** Complete - reviewed, guarded and machine-verified (see §7)
> **Milestone:** [Phase 7 - Hardening, packaging, release](https://github.com/mahmoudfarahat2647/coderadar/milestone/8)
> **Closes task line:** Phase 7 checklist item *Security review against NFR-05 (path traversal, CSP, keychain)* of [`PHASES.md`](../PHASES.md)
> **Companion:** [`01-BRD-AND-MVP-REQUIREMENTS.md`](./01-BRD-AND-MVP-REQUIREMENTS.md) (§4, NFR-05), [`02-CLEAN-ARCHITECTURE-BLUEPRINT.md`](./02-CLEAN-ARCHITECTURE-BLUEPRINT.md), [`13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md`](./13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md)
> **Gate:** *"Every NFR-05 control is enforced by a test that fails the build when it regresses."*

---

## 1. Goal

NFR-05 (`CR-SPEC-01` §4) states three controls for the desktop application:

> **Path Traversal Guard:** file writes are restricted to the selected workspace root; attempts to write outside or follow symlinks escaping root are blocked.
> **Keychain Secret Storage:** API keys are stored in the OS Keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service via `keyring-rs`), never plaintext files.
> **Webview CSP:** strict Content Security Policy with disabled external script execution.

A requirement that only lives in a document is a wish. This review turns NFR-05 into
**enforced, machine-checkable guards**:

| Deliverable | Path |
| :--- | :--- |
| Shell guard (CI, `ubuntu-latest`) | `scripts/check-security.sh` |
| Cross-platform Rust twin (every machine) | `crates/coderadar-infrastructure/tests/security_posture.rs` |
| This review | `coderadar-specs/14-PHASE-7-SECURITY-REVIEW.md` |

Both guards assert the same seven invariants. The shell guard fails the CI job with a red
`[security] VIOLATION:` line; the Rust twin fails `cargo test` on the developer's machine.

## 2. Scope

### 2.1 In scope — the three NFR-05 controls

1. **Path traversal guard** — every write into the user's workspace is contained by
   `WorkspaceGuard`.
2. **Keychain secret storage** — the Jev / OpenRouter API key lives in the OS credential
   store and never in a plaintext file, in the web storage or in the repository.
3. **Strict webview CSP** — the policy forbids every remote origin and every inline/eval
   script.

### 2.2 In scope — the two controls this review adds

4. **No shell injection surface** — the product never spawns a process, so a compromised
   webview or a malicious scan finding can never reach a shell.
5. **No plaintext secret persistence** — the API key is *only* handled by the OS keychain
   and held in memory by the frontend.

### 2.3 Out of scope

* The OS-level sandboxing of the webview process (a webview-engine concern, not an
  application one) — see §6.
* The privacy layer of NFR-02 (secret redaction before egress), which is covered by
  `CR-SPEC-12 §3` and tested by `crates/coderadar-infrastructure/src/redaction.rs`.
* The packaging / release work of the rest of Phase 7.

## 3. What was actually implemented (before this review)

| Control | Implementation | Verified by |
| :--- | :--- | :--- |
| Traversal / symlink escape | `WorkspaceGuard::resolve` rejects empty, absolute, `..`-bearing and symlink-escaping paths (`crates/coderadar-infrastructure/src/workspace_guard.rs:82`) | unit tests in the same file, `crates/coderadar-infrastructure/tests/patch_safety.rs` |
| Atomic write | `atomic_write` stages next to the target with `create_new`, then renames (`crates/coderadar-infrastructure/src/patching.rs:71`, `crates/coderadar-infrastructure/src/patching.rs:115`) | `crates/coderadar-infrastructure/tests/patch_safety.rs` |
| Optimistic concurrency | SHA-256 of the target must match the preview (`crates/coderadar-infrastructure/src/patching.rs:228`) | `crates/coderadar-infrastructure/tests/patch_safety.rs` |
| Keychain | `KeyringCredentialStore` over `keyring-rs`, service `coderadar-studio`, account `jev-api-key` (`crates/coderadar-infrastructure/src/keychain.rs:19`, `crates/coderadar-infrastructure/src/keychain.rs:96`) | `crates/coderadar-infrastructure/src/keychain.rs` unit tests |
| Secret redaction | `redact_source` scrubs credential-shaped literals (`crates/coderadar-infrastructure/src/redaction.rs:31`) | unit tests in the same file |
| CSP | `app.security.csp` (`src-tauri/tauri.conf.json:25`) | **nothing — this review adds it** |
| ACL | `src-tauri/capabilities/default.json:6` | **nothing — this review adds it** |
| No shell / no plaintext secret | — | **nothing — this review adds it** |

## 4. Threat model and controls

The product is a desktop application that (a) reads a user's workspace, (b) writes to it
on the user's behalf, and (c) renders a React UI in a system webview. The interesting
attack surfaces are therefore:

* **AI-generated content.** A finding may carry a file path and a patch. If the path is
  attacker-controlled, it is a remote-code-execution primitive on the user's machine.
* **The webview.** The webview is the only part of the application that parses untrusted
  input (the rendered findings). A CSP hole or a remote script turns the webview into a
  remote-code-execution primitive.
* **The IPC boundary.** The frontend sends the API key and the report target path to the
  Rust backend. A compromised webview would try to persist the key or redirect the write.

| # | Threat | Control | Implemented in | Verified by |
| :--- | :--- | :--- | :--- | :--- |
| T-1 | A malicious finding writes `../../.ssh/authorized_keys` | Absolute paths and `..` components are refused before any I/O | `crates/coderadar-infrastructure/src/workspace_guard.rs:90`, `crates/coderadar-infrastructure/src/workspace_guard.rs:103` | `security_posture.rs::every_workspace_write_goes_through_the_workspace_guard`; `workspace_guard.rs` unit tests |
| T-2 | A symlink inside the workspace points at `/etc` | The canonical parent and the canonical target must stay under the canonical root | `crates/coderadar-infrastructure/src/workspace_guard.rs:118`, `crates/coderadar-infrastructure/src/workspace_guard.rs:138` | `workspace_guard.rs` unit tests (`a_symlink_pointing_outside_the_workspace_is_rejected`) |
| T-3 | A write lands on a path planted by somebody else | The staging file is created with `create_new` and then renamed | `crates/coderadar-infrastructure/src/patching.rs:71`, `crates/coderadar-infrastructure/src/patching.rs:155` | `patching.rs` unit tests (`a_planted_symlink_is_never_followed`) |
| T-4 | A stale diff overwrites a file the user just edited | The SHA-256 fingerprint of the preview must still match | `crates/coderadar-infrastructure/src/patching.rs:228` | `patch_safety.rs` |
| T-5 | A path is written that never went through the guard | `atomic_write` is `pub(crate)` and has a closed set of callers | `crates/coderadar-infrastructure/src/patching.rs:115`, `crates/coderadar-infrastructure/src/lib.rs:66` | `security_posture.rs::every_workspace_write_goes_through_the_workspace_guard` |
| T-6 | The API key is written to a plaintext file or to the web storage | The OS keychain is the only persistence point; the shell writes no file; the frontend keeps the key in memory | `crates/coderadar-infrastructure/src/keychain.rs:96`, `src-tauri/src/commands.rs:286`, `src/components/ApiKeyDialog.tsx:22` | `security_posture.rs::the_api_key_is_never_persisted_in_plaintext` |
| T-7 | The key leaks through the UI or the logs | Only a 6-character masked prefix is returned to the frontend | `crates/coderadar-infrastructure/src/keychain.rs:201`, `src-tauri/src/commands.rs:168` | `keychain.rs` unit tests (`mask_keeps_a_short_prefix`) |
| T-8 | A secret reaches the Jev / OpenRouter endpoint | Tier 0 redaction scrubs credential-shaped literals before egress | `crates/coderadar-infrastructure/src/redaction.rs:31` | `redaction.rs` unit tests |
| T-9 | A remote script runs in the webview | The CSP forbids every remote origin, `unsafe-eval` and inline script | `src-tauri/tauri.conf.json:25` | `security_posture.rs::the_webview_csp_is_strict_and_free_of_any_remote_origin`; `scripts/check-security.sh` |
| T-10 | A remote stylesheet / font leaks the workspace usage to a third party | `index.html` and the frontend load no remote resource | `index.html:7`, `src/**` | `security_posture.rs::the_webview_loads_no_remote_resource`; `scripts/check-security.sh` |
| T-11 | A compromised webview reaches the filesystem, a shell or the network | The Tauri ACL grants no `fs:`, `shell:`, `process:`, `http:` or `oci:` permission | `src-tauri/capabilities/default.json:6` | `security_posture.rs::the_tauri_acl_grants_no_dangerous_permission`; `scripts/check-security.sh` |
| T-12 | A malicious finding triggers a process launch (shell injection) | The product sources contain no process-spawning API and no shell interpreter | `crates/coderadar-*/src`, `src-tauri/src` | `security_posture.rs::the_product_sources_never_spawn_a_process`, `security_posture.rs::no_shell_interpreter_is_invoked`; `scripts/check-security.sh` |

## 5. The enforced invariants

`scripts/check-security.sh` and `crates/coderadar-infrastructure/tests/security_posture.rs`
assert the same seven invariants. Each one is a separate `#[test]` in the Rust twin, so a
regression names the broken control in the test output.

| # | Invariant | Shell check | Rust test |
| :--- | :--- | :--- | :--- |
| 1 | The product sources never spawn a process (`std::process::Command`, `Command::new`, `process::Command`) | `product_matches` | `the_product_sources_never_spawn_a_process` |
| 2 | Neither the backend nor the frontend invokes a shell (`sh -c`, `bash -c`, `cmd /C`, `cmd.exe /C`, `powershell -Command`, `powershell -EncodedCommand`, `eval(`) | `product_matches` + `frontend_matches` | `no_shell_interpreter_is_invoked` |
| 3 | The CSP contains `default-src 'self'`, `script-src 'self'`, `object-src 'none'`, `base-uri 'none'`, `frame-ancestors 'none'`, `form-action 'none'`; no `unsafe-eval`; no `unsafe-inline` in `script-src`; no remote origin (only `http://ipc.localhost` is allowed) | `csp_value` / `csp_directive` | `the_webview_csp_is_strict_and_free_of_any_remote_origin` |
| 4 | The ACL grants no `fs:` / `shell:` / `process:` / `http:` / `oci:` permission and keeps `dialog:default` | `capability_permissions` | `the_tauri_acl_grants_no_dangerous_permission` |
| 5 | `index.html` contains no absolute URL, and the frontend loads no remote resource (module import, `fetch`, CSS `url()`, tag `src=`, `@import`, `<link href>`) | `REMOTE_RESOURCE_PATTERN` | `the_webview_loads_no_remote_resource` |
| 6 | The Tauri shell writes no file; the OS keychain is the only persistence point of the credential (`keyring::Entry` + `set_password`); a crate never writes a file with `fs::write` outside a test module; the frontend uses no `localStorage` / `sessionStorage` / `indexedDB` / cookie | `crate_matches` | `the_api_key_is_never_persisted_in_plaintext` |
| 7 | The workspace guard exists and resolves, the patch applier uses it, `atomic_write` stays `pub(crate)` and is not re-exported, and only `patching.rs` / `export.rs` may call it | `grep` on the four files | `every_workspace_write_goes_through_the_workspace_guard` |

### 5.1 Design notes

* **Self-trip protection.** The guards name what they forbid. Both the shell script and
  the Rust test therefore assemble the forbidden tokens at run time from fragments
  (`"std::process:" + "Command"`, `"sh " + "-c"`, …) and the greps skip the `tests/` and
  `scripts/` directories. A guard can never flag itself.
* **No new dependency.** The Rust twin uses `std` only — it does not add a dev-dependency
  and does not even use `serde_json`: the two JSON files are read with a small, targeted
  extractor (`json_string_field` / `json_string_array`).
* **Idempotence and CWD independence.** The shell guard resolves the repository root from
  `BASH_SOURCE`, writes nothing, and can be run from any directory and any number of
  times.
* **Forward compatibility.** The ACL check forbids dangerous scopes but does not require
  `dialog:allow-save`; the dialog permission set may grow without touching the guard.

## 6. Findings of this review

| ID | Severity | Finding | Status |
| :--- | :--- | :--- | :--- |
| F-1 | **High** | The CSP did not forbid `object-src`, `base-uri`, `frame-ancestors`, `frame-src` or `form-action`, and `index.html` loaded the Inter / Manrope / JetBrains Mono web fonts from `fonts.googleapis.com` / `fonts.gstatic.com`. A remote origin in the policy is a remote origin in the product: it breaks the offline-first promise (NFR-03) and hands the workspace metadata to a third party. | **Fixed in this PR** — the policy is now `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data: asset:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; frame-src 'none'; form-action 'none'` and the web fonts were replaced by the system font stack. |
| F-2 | **Medium** | Nothing prevented a future change from adding a `fs:` / `shell:` / `http:` permission to the Tauri ACL, or from loosening the CSP. NFR-05 was a *document*, not a *gate*. | **Fixed in this PR** — `scripts/check-security.sh` and `security_posture.rs`. |
| F-3 | **Medium** | The product could have gained a process-spawning call (`std::process::Command`) without any test failing. On a desktop app that rewrites the user's files, a shell-reachable path is a remote-code-execution primitive. | **Fixed in this PR** — invariant 1 and 2. |
| F-4 | **Medium** | The Markdown export (`export_report`, `crates/coderadar-infrastructure/src/export.rs:125`) writes through `atomic_write` **without** going through `WorkspaceGuard`: the target path is an arbitrary string coming from the webview over IPC. This is by design for a "save as" feature — the user picks the destination — but it means the workspace guard does *not* contain every write of the application. | **Accepted and documented** (see §6.1). The guard now enforces a closed set of callers for `atomic_write` (`patching.rs`, `export.rs`) so that a third writer cannot appear unnoticed. |
| F-5 | **Low** | `src/components/Header.tsx:153` carries an external hyperlink (`https://github.com/mahmoudfarahat2647/coderadar`) with `target="_blank"`. It is not a resource load, so it does not defeat the CSP, but the webview could be navigated away from the application by a click. | **Accepted** (see §6.1). `shell:allow-open` is deliberately **not** granted, so there is no safe way to hand the URL to the OS browser; the recommendation is to drop the anchor or open it from a Rust command in Phase 8. |
| F-6 | **Low** | `src/components/FindingDrawer.tsx:40` and `:42` contain `http://api.internal.service` and `https://api.internal.service` inside a *quoted example diff* rendered in the UI. They are inert strings, not requests. | **Accepted.** The guards detect *resource loads*, not inert strings, so a realistic example is not reported as a breach. |
| F-7 | **Info** | The `keyring` backend is per-platform (`windows-native`, `apple-native`, `linux-native`). On Linux the credential store is the kernel keyring, which requires a Secret Service provider such as `gnome-keyring` or `kwallet` to be running; on a headless box the store is simply unavailable and the application degrades to the Mock engine. | **Accepted** — see §7. |

### 6.1 The one deviation from the literal NFR-05 wording

NFR-05 says *file writes are restricted to the selected workspace root*. That is exactly
what the patch applier does (`crates/coderadar-infrastructure/src/patching.rs:195` and
`:204`: `WorkspaceGuard::new(root)` then `guard.resolve(&file_path)`). It is **not** what
the Markdown export does: the export target is chosen by the user in the OS save dialog
and is written verbatim. Containing the export inside the workspace root would make the
"export a report" feature useless.

The enforced invariant is therefore the precise and honest version:

> Every write **into the user's workspace** goes through `WorkspaceGuard`; the only other
> writer is the Markdown export, whose target is an explicit path chosen by the user (the
> OS save dialog or the CLI `--out`); and the atomic write helper is crate-private with a
> closed set of callers.

This is why invariant 7 checks the *callers* of `atomic_write` instead of assuming there is
only one.

## 7. Residual risks and known limits

1. **`style-src` still needs `'unsafe-inline'`.** The radar overlay sets the polar
   geometry of each node through React inline styles (`style={{ left, top, … }}`), and the
   Tailwind build emits inline `style` attributes. Removing `'unsafe-inline'` from
   `style-src` would require a CSS nonce or a `style` hash per document, which the current
   build does not produce. Inline *script* is forbidden; inline *style* is not.
2. **The webview process is not sandboxed by the OS beyond the CSP.** Tauri renders in
   the platform webview (WebView2 / WKWebView / WebKitGTK); the application does not and
   cannot disable the engine's own sandbox. The CSP is the application's own boundary.
3. **The keyring backend needs a Secret Service provider on Linux.** Without
   `gnome-keyring` / `kwallet` running, the store is unavailable; the application then
   falls back to the `JEV_API_KEY` environment variable or to the Mock engine. This is a
   degraded mode, not a silent plaintext fallback — no key is ever written to disk.
4. **The export path is webview-controlled (F-4).** A compromised webview could ask the
   backend to write a Markdown file anywhere the user can write. The CSP is the control
   that keeps the webview uncompromised; the Phase 8 recommendation is to move the save
   dialog into the Rust command so the target never crosses the IPC boundary as a string.
5. **The guards are lexical, not semantic.** They detect the *tokens* that would open a
   hole, not the intent. A sufficiently creative spelling (`std::process::` + `Command`
   built at run time) would pass. The invariant is a strong signal, not a proof; the
   human review of the diff remains the last line of defence.
6. **`fs::write` outside a test module is detected heuristically** (the file must contain a
   `#[cfg(test)]` marker). A file that mixes a production write with a test module would
   not be caught by that particular rule.

## 8. Verification

Run both guards locally:

```bash
bash scripts/check-security.sh
cargo test -p coderadar-infrastructure --test security_posture
```

The shell guard prints one `[security] checked: …` line per invariant and, when the
posture holds, a green summary:

```
[security] checked: no process is spawned by the product code
[security] checked: no shell interpreter is invoked
[security] checked: the webview CSP is strict and free of any remote origin
[security] checked: the Tauri ACL grants no dangerous permission
[security] checked: the webview loads no remote resource
[security] checked: the API key is never persisted in plaintext
[security] checked: the patch applier resolves every workspace write through the workspace guard; the Markdown export writes only the user-chosen report path
[security] OK: no process spawn, no shell interpreter, strict local CSP, minimal ACL, no remote resource, no plaintext secret, patch-applied workspace writes guarded.
```

A breach prints a red `[security] VIOLATION:` line per problem and exits `1`:

```
[security] VIOLATION: the CSP allows 'unsafe-inline' in 'script-src'
```

### 8.1 CI wiring

`scripts/check-security.sh` is designed to run in a dedicated CI job next to the existing
architecture job (`.github/workflows/ci.yml`, `jobs.architecture`):

```yaml
  security:
    name: Desktop AppSec posture
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Enforce the NFR-05 Desktop AppSec posture
        run: bash scripts/check-security.sh
```

The Rust twin needs no CI wiring: it is part of `cargo test --workspace --all-targets`.

## 9. Non-goals

* No OS-level sandboxing of the webview (an engine concern, see §7.2).
* No signed rule updates, no plugin system (Phase 8).
* No hardening of the Jev HTTP client beyond what `CR-SPEC-12` already specifies.
