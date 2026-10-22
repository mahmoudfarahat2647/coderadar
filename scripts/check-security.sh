#!/usr/bin/env bash
#
# Enforces the Desktop AppSec posture required by NFR-05 (`CR-SPEC-01` §4):
#
#   1. path traversal guard   — every patch-applied workspace write is contained;
#   2. keychain secret storage — the API key is never written in plaintext;
#   3. strict webview CSP     — no remote origin, no inline/eval script;
#
# plus the two controls the Phase 7 security review adds on top of NFR-05:
#
#   4. no shell injection surface — the product never spawns a process;
#   5. no plaintext secret persistence.
#
# The script only needs `bash`, `grep`, `awk` and `sed`. It resolves the
# repository root from `BASH_SOURCE`, so it can be run from any directory.
#
# The cross-platform twin of this guard is
# `crates/coderadar-infrastructure/tests/security_posture.rs`, which runs on
# every developer machine (Windows, macOS, Linux) and in CI.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAIL=0

# The product sources: everything that ships inside the desktop binary.
PRODUCT_DIRS=(
  "$ROOT/crates/coderadar-domain/src"
  "$ROOT/crates/coderadar-usecases/src"
  "$ROOT/crates/coderadar-adapters/src"
  "$ROOT/crates/coderadar-infrastructure/src"
  "$ROOT/src-tauri/src"
)

# The crate sources (Layer 1 - Layer 4) of the cargo workspace.
CRATE_DIRS=(
  "$ROOT/crates/coderadar-domain/src"
  "$ROOT/crates/coderadar-usecases/src"
  "$ROOT/crates/coderadar-adapters/src"
  "$ROOT/crates/coderadar-infrastructure/src"
)

FRONTEND_DIR="$ROOT/src"
INDEX_HTML="$ROOT/index.html"
TAURI_CONF="$ROOT/src-tauri/tauri.conf.json"
CAPABILITIES="$ROOT/src-tauri/capabilities/default.json"

# Fail with a red message.
violation() {
  printf '\033[31m[security] VIOLATION:\033[0m %s\n' "$1" >&2
  FAIL=1
}

# Announce a check that passed.
checked() {
  printf '[security] checked: %s\n' "$1"
}

# ---------------------------------------------------------------------------
# The forbidden tokens are assembled at run time from fragments, so this
# guard never contains a complete forbidden token in its own source. The
# greps below also skip the `tests/` and `scripts/` directories for the same
# reason: the guards themselves are allowed to *name* what they forbid.
# ---------------------------------------------------------------------------
NEEDLE_PROCESS_FULL="std::process:"'Command'
NEEDLE_PROCESS_NEW='Command''::new'
NEEDLE_PROCESS_SHORT="process:"'Command'

NEEDLE_SH_C="sh ""-c"
NEEDLE_BASH_C="bash ""-c"
NEEDLE_CMD_C="cmd ""/C"
NEEDLE_CMD_EXE_C="cmd.exe ""/C"
NEEDLE_PS_COMMAND="powershell ""-Command"
NEEDLE_PS_ENCODED="powershell ""-EncodedCommand"
NEEDLE_EVAL="(^|[^A-Za-z0-9_])eval""[[:space:]]*\("

# A remote resource is a URL that is *loaded* by the webview: a module
# import, a fetch, a CSS `url()`, a tag `src=`, a CSS `@import` or a
# `<link>` stylesheet. An inert hyperlink is not a resource load.
REMOTE_RESOURCE_PATTERN="(from[[:space:]]*['\"]https?://)|(import[[:space:]]*\([[:space:]]*['\"]https?://)|(fetch[[:space:]]*\([[:space:]]*['\"]https?://)|(url[[:space:]]*\([[:space:]]*['\"]?https?://)|(src[[:space:]]*=[[:space:]]*['\"]https?://)|(@import[[:space:]]+['\"]https?://)|(<link[^>]*href[[:space:]]*=[[:space:]]*['\"]https?://)"

# Print every match of the extended regular expression `$1` inside the
# product sources (`path:line:content`).
product_matches() {
  local pattern="$1"
  local dir
  for dir in "${PRODUCT_DIRS[@]}"; do
    [ -d "$dir" ] || continue
    grep -rnE --exclude-dir=tests --exclude-dir=scripts -- "$pattern" "$dir" 2>/dev/null || true
  done
}

# Print every match of the extended regular expression `$1` inside the
# crate sources of the cargo workspace.
crate_matches() {
  local pattern="$1"
  local dir
  for dir in "${CRATE_DIRS[@]}"; do
    [ -d "$dir" ] || continue
    grep -rnE --exclude-dir=tests --exclude-dir=scripts -- "$pattern" "$dir" 2>/dev/null || true
  done
}

# Print every match of the extended regular expression `$1` inside the
# frontend sources (`src/`) and inside `index.html`.
frontend_matches() {
  local pattern="$1"
  if [ -d "$FRONTEND_DIR" ]; then
    grep -rnE --exclude-dir=node_modules --exclude-dir=dist -- "$pattern" "$FRONTEND_DIR" 2>/dev/null || true
  fi
  if [ -f "$INDEX_HTML" ]; then
    grep -nE -- "$pattern" "$INDEX_HTML" 2>/dev/null || true
  fi
}

# Print the value of the `"csp"` field of the Tauri configuration.
csp_value() {
  sed -n 's/.*"csp"[[:space:]]*:[[:space:]]*"\(.*\)".*/\1/p' "$TAURI_CONF"
}

# Print the value of one directive of a CSP policy.
csp_directive() {
  printf '%s' "$1" | tr ';' '\n' | sed -n "s/^[[:space:]]*$2[[:space:]]*//p"
}

# Print one entry per line of the `permissions` array of the Tauri ACL.
capability_permissions() {
  awk '
    /"permissions"[[:space:]]*:[[:space:]]*\[/ { inperms = 1; next }
    inperms {
      line = $0
      while (match(line, /"[^"]*"/)) {
        print substr(line, RSTART + 1, RLENGTH - 2)
        line = substr(line, RSTART + RLENGTH)
      }
      if ($0 ~ /\]/) { inperms = 0 }
    }
  ' "$1"
}

# ---------------------------------------------------------------------------
# 1. No process is spawned by the product code.
# ---------------------------------------------------------------------------
spawn_matches="$(product_matches "$NEEDLE_PROCESS_FULL|$NEEDLE_PROCESS_NEW|$NEEDLE_PROCESS_SHORT")"
if [ -n "$spawn_matches" ]; then
  violation "the product sources spawn a process; the desktop app must never start an external command:
$spawn_matches"
fi
checked "no process is spawned by the product code"

# ---------------------------------------------------------------------------
# 2. No shell interpreter is invoked, in the backend or in the webview.
# ---------------------------------------------------------------------------
shell_pattern="$NEEDLE_SH_C|$NEEDLE_BASH_C|$NEEDLE_CMD_C|$NEEDLE_CMD_EXE_C|$NEEDLE_PS_COMMAND|$NEEDLE_PS_ENCODED|$NEEDLE_EVAL"
shell_matches="$(product_matches "$shell_pattern")"
if [ -n "$shell_matches" ]; then
  violation "the product sources invoke a shell interpreter:
$shell_matches"
fi
frontend_shell_matches="$(frontend_matches "$shell_pattern")"
if [ -n "$frontend_shell_matches" ]; then
  violation "the frontend invokes a shell interpreter:
$frontend_shell_matches"
fi
checked "no shell interpreter is invoked"

# ---------------------------------------------------------------------------
# 3. The webview Content Security Policy is strict and fully local.
# ---------------------------------------------------------------------------
if [ ! -f "$TAURI_CONF" ]; then
  violation "the Tauri configuration is missing: $TAURI_CONF"
else
  CSP="$(csp_value)"
  if [ -z "$CSP" ]; then
    violation "app.security.csp is missing from $TAURI_CONF"
  else
    for directive in \
      "default-src 'self'" \
      "script-src 'self'" \
      "object-src 'none'" \
      "base-uri 'none'" \
      "frame-ancestors 'none'" \
      "form-action 'none'"
    do
      case "$CSP" in
        *"$directive"*) ;;
        *) violation "the CSP does not contain '$directive'" ;;
      esac
    done

    case "$CSP" in
      *unsafe-eval*) violation "the CSP allows 'unsafe-eval'" ;;
    esac

    SCRIPT_SRC="$(csp_directive "$CSP" "script-src")"
    case "$SCRIPT_SRC" in
      *unsafe-inline*) violation "the CSP allows 'unsafe-inline' in 'script-src'" ;;
    esac

    case "$CSP" in
      *https://*) violation "the CSP references a remote origin (https://)" ;;
    esac

    # The only remote-looking origin allowed by the policy is the local
    # Tauri IPC endpoint.
    CSP_WITHOUT_IPC="$(printf '%s' "$CSP" | sed 's|http://ipc\.localhost||g')"
    case "$CSP_WITHOUT_IPC" in
      *http://*)
        OFFENDER="$(printf '%s' "$CSP_WITHOUT_IPC" | sed -n 's|.*\(http://[^[:space:];]*\).*|\1|p')"
        violation "the CSP references a remote origin ($OFFENDER)"
        ;;
    esac
  fi
fi
checked "the webview CSP is strict and free of any remote origin"

# ---------------------------------------------------------------------------
# 4. The Tauri ACL grants no filesystem, shell, process, http or oci
#    permission, and keeps the dialog it needs.
# ---------------------------------------------------------------------------
if [ ! -f "$CAPABILITIES" ]; then
  violation "the Tauri capability file is missing: $CAPABILITIES"
else
  SAW_DIALOG_DEFAULT=0
  while IFS= read -r permission; do
    [ -z "$permission" ] && continue
    case "$permission" in
      fs:*|shell:*|process:*|http:*|oci:*)
        violation "the capability set grants '$permission'; the desktop app must not hold filesystem, shell, process, http or oci permissions"
        ;;
    esac
    if [ "$permission" = "dialog:default" ]; then
      SAW_DIALOG_DEFAULT=1
    fi
  done <<EOF
$(capability_permissions "$CAPABILITIES")
EOF
  if [ "$SAW_DIALOG_DEFAULT" -ne 1 ]; then
    violation "the capability set must keep 'dialog:default'"
  fi
fi
checked "the Tauri ACL grants no dangerous permission"

# ---------------------------------------------------------------------------
# 5. The webview loads no remote resource.
# ---------------------------------------------------------------------------
if [ -f "$INDEX_HTML" ]; then
  INDEX_URLS="$(grep -nE 'https?://' "$INDEX_HTML" || true)"
  if [ -n "$INDEX_URLS" ]; then
    violation "index.html references a remote URL:
$INDEX_URLS"
  fi
else
  violation "index.html is missing: $INDEX_HTML"
fi

remote_matches="$(frontend_matches "$REMOTE_RESOURCE_PATTERN")"
if [ -n "$remote_matches" ]; then
  violation "the webview loads a remote resource:
$remote_matches"
fi
checked "the webview loads no remote resource"

# ---------------------------------------------------------------------------
# 6. The API key is never persisted in plaintext.
# ---------------------------------------------------------------------------
SHELL_WRITE_PATTERN="(fs::write)|(write_all[[:space:]]*\()|(File::create)|(OpenOptions)|(fs::remove_(file|dir))|(fs::rename)|(fs::copy)"
shell_write_matches="$(grep -rnE -- "$SHELL_WRITE_PATTERN" "$ROOT/src-tauri/src" 2>/dev/null || true)"
if [ -n "$shell_write_matches" ]; then
  violation "the Tauri shell writes to the filesystem; the API key must only reach the OS keychain:
$shell_write_matches"
fi

KEYCHAIN="$ROOT/crates/coderadar-infrastructure/src/keychain.rs"
if [ ! -f "$KEYCHAIN" ]; then
  violation "the keychain store is missing: $KEYCHAIN"
else
  if ! grep -qF 'keyring::Entry' "$KEYCHAIN"; then
    violation "the keychain store does not use the OS credential store (keyring::Entry)"
  fi
  if ! grep -qF 'set_password' "$KEYCHAIN"; then
    violation "the keychain store does not persist the credential (keyring set_password)"
  fi
fi

# The credential store is the only place where the API key may be persisted.
persistence_matches="$(crate_matches "set_password")"
while IFS= read -r hit; do
  [ -z "$hit" ] && continue
  case "$hit" in
    *"/keychain.rs:"*) ;;
    *) violation "the API key is persisted outside the keychain store: $hit" ;;
  esac
done <<EOF
$persistence_matches
EOF

# A direct filesystem write is only allowed inside a `#[cfg(test)]` module.
while IFS= read -r hit; do
  [ -z "$hit" ] && continue
  FILE="${hit%%:*}"
  if ! grep -qF '#[cfg(test)]' "$FILE"; then
    violation "$FILE writes a file with fs::write outside a test module"
  fi
done <<EOF
$(crate_matches "fs::write")
EOF

# The frontend keeps the key in memory only.
STORAGE_PATTERN="(localStorage)|(sessionStorage)|(indexedDB)|(IndexedDB)|(document\.cookie)"
storage_matches="$(frontend_matches "$STORAGE_PATTERN")"
if [ -n "$storage_matches" ]; then
  violation "the frontend persists the API key in the web storage:
$storage_matches"
fi
checked "the API key is never persisted in plaintext"

# ---------------------------------------------------------------------------
# 7. The workspace guard contains every patch-applied workspace write.
# ---------------------------------------------------------------------------
GUARD="$ROOT/crates/coderadar-infrastructure/src/workspace_guard.rs"
PATCHING="$ROOT/crates/coderadar-infrastructure/src/patching.rs"
INFRA_LIB="$ROOT/crates/coderadar-infrastructure/src/lib.rs"

if [ ! -f "$GUARD" ]; then
  violation "the workspace guard is missing: $GUARD"
else
  if ! grep -qF 'pub struct WorkspaceGuard' "$GUARD"; then
    violation "the workspace guard does not declare 'pub struct WorkspaceGuard'"
  fi
  if ! grep -qE 'pub fn resolve' "$GUARD"; then
    violation "the workspace guard does not expose a 'resolve' method"
  fi
fi

if [ ! -f "$PATCHING" ]; then
  violation "the patch applier is missing: $PATCHING"
else
  if ! grep -qF 'WorkspaceGuard' "$PATCHING"; then
    violation "the patch applier does not use the workspace guard"
  fi
  if ! grep -qE '\.resolve\(' "$PATCHING"; then
    violation "the patch applier does not resolve the target through the workspace guard"
  fi
  if ! grep -qE 'pub\(crate\)[[:space:]]+fn[[:space:]]+atomic_write' "$PATCHING"; then
    violation "'atomic_write' must stay 'pub(crate)' in patching.rs: it is the guarded write primitive"
  fi
  if grep -qE 'pub[[:space:]]+fn[[:space:]]+atomic_write' "$PATCHING"; then
    violation "'atomic_write' is 'pub' in patching.rs: it must not be part of the public API"
  fi
fi

if [ -f "$INFRA_LIB" ]; then
  if grep -qE 'pub[[:space:]]+use[^;]*atomic_write' "$INFRA_LIB"; then
    violation "'atomic_write' is re-exported by the infrastructure crate: it must stay crate-private"
  fi
fi

# The atomic write helper has a closed set of callers: the patch applier
# (which resolves every workspace write through the workspace guard), the
# Markdown export (which writes only the user-chosen report path) and the
# signed-rules installer (which writes only the verified manifest under the
# app-data directory, never inside a workspace).
while IFS= read -r hit; do
  [ -z "$hit" ] && continue
  FILE="${hit%%:*}"
  BASE="${FILE##*/}"
  case "$BASE" in
    patching.rs|export.rs|rules_ota.rs) ;;
    *) violation "$BASE calls 'atomic_write' outside the sanctioned writers (patching.rs, export.rs, rules_ota.rs)" ;;
  esac
done <<EOF
$(crate_matches "atomic_write")
EOF
checked "the patch applier resolves every workspace write through the workspace guard; the Markdown export writes only the user-chosen report path; the rules installer writes only a verified manifest outside workspaces"

# ---------------------------------------------------------------------------
if [ "$FAIL" -ne 0 ]; then
  printf '\033[31m[security] Desktop AppSec posture broken.\033[0m\n' >&2
  exit 1
fi

printf '\033[32m[security] OK:\033[0m no process spawn, no shell interpreter, strict local CSP, minimal ACL, no remote resource, no plaintext secret, patch-applied workspace writes guarded.\n'
