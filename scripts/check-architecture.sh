#!/usr/bin/env bash
#
# Enforces the Clean Architecture Dependency Rule (CR-SPEC-02 §3).
#
# Fails the build when:
#   * `coderadar-domain` declares ANY dependency (std only);
#   * `coderadar-usecases` declares anything other than `coderadar-domain`
#     (in particular: no `tokio`, no async runtime, no outer layer);
#   * either inner crate references a forbidden crate in its source.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOMAIN="$ROOT/crates/coderadar-domain"
USECASES="$ROOT/crates/coderadar-usecases"
FAIL=0

# Print the dependency keys declared in a Cargo.toml dependency table.
# Usage: declared_deps <cargo.toml> <table-name>
declared_deps() {
  local file="$1" table="$2"
  awk -v want="[$table]" '
    /^\[/ {
      header = $0
      sub(/[ \t].*/, "", header)
      in_table = (header == want)
      next
    }
    in_table {
      line = $0
      sub(/#.*/, "", line)
      gsub(/^[ \t]+|[ \t]+$/, "", line)
      if (line == "") next
      split(line, parts, "=")
      key = parts[1]
      gsub(/[ \t]+$/, "", key)
      print key
    }
  ' "$file"
}

# Fail with a red message.
violation() {
  printf '\033[31m[architecture] VIOLATION:\033[0m %s\n' "$1" >&2
  FAIL=1
}

# --- 1. coderadar-domain must declare zero dependencies -----------------------
for table in dependencies dev-dependencies build-dependencies; do
  while IFS= read -r dep; do
    [ -z "$dep" ] && continue
    violation "coderadar-domain declares '$dep' in [$table]; the domain core must be std-only"
  done < <(declared_deps "$DOMAIN/Cargo.toml" "$table")
done

# --- 2. coderadar-usecases may only depend on coderadar-domain ----------------
for table in dependencies dev-dependencies build-dependencies; do
  while IFS= read -r dep; do
    [ -z "$dep" ] && continue
    if [ "$dep" != "coderadar-domain" ]; then
      violation "coderadar-usecases declares '$dep' in [$table]; only coderadar-domain is allowed"
    fi
  done < <(declared_deps "$USECASES/Cargo.toml" "$table")
done

# --- 3. No forbidden tokens anywhere in the inner crates' manifests -----------
for manifest in "$DOMAIN/Cargo.toml" "$USECASES/Cargo.toml"; do
  # Strip full-line comments so documentation may freely name the forbidden
  # crates without tripping the guard.
  if grep -vE '^[[:space:]]*#' "$manifest" \
    | grep -Eq 'tokio|async-std|smol|reqwest|sqlx|coderadar-adapters|coderadar-infrastructure'; then
    violation "$manifest mentions a forbidden dependency token"
  fi
done

# --- 4. No forbidden imports in the inner crates' source ----------------------
for src in "$DOMAIN/src" "$USECASES/src"; do
  if grep -rEq '\b(use|extern crate)[[:space:];:]+(tokio|async_std|smol|reqwest|sqlx)\b' "$src"; then
    violation "$src imports an async runtime or driver crate"
  fi
  if grep -rEq '\bcoderadar_(adapters|infrastructure)\b' "$src"; then
    violation "$src references an outer-layer crate"
  fi
done

if [ "$FAIL" -ne 0 ]; then
  printf '\033[31m[architecture] Dependency Rule broken.\033[0m\n' >&2
  exit 1
fi

printf '\033[32m[architecture] OK:\033[0m domain is std-only and usecases depends only on domain.\n'
