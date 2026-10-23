#!/usr/bin/env bash
# spec-template.sh — select specs and snapshot staged content for the shared
# validator in spec-template.ts. The native orly spec.gate criterion calls the
# same validator directly; this entry point serves script callers.
# Dispatch façade: dispatch/write_spec.md (SPEC TEMPLATE GATE).
#
# Modes:
#   --staged         diff-scope: only specs in `git diff --cached`
#   --all            (default) pending+active specs only — current/in-flight work
#   --include-done   adds done/ specs to the scan (one-time sweep tool)
#
# Staged/file checks require complete sections, references, test mappings and
# declared commands. Bulk scans check prohibited patterns only. Bun is required
# for both scopes; bulk checks do not require repository command configuration.
#
# Done specs are excluded by default: historical artifacts, not work product.
# Exits 0 clean, 1 on any blocking finding.

set -euo pipefail

MODE="${1:-${SCOPE:-all}}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

FAIL=0
fail() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()   { printf "OK:   %s\n" "$*"; }

# Discover spec files in scope.
case "$MODE" in
  --staged|staged)
    # while read, not mapfile — bash-3.2 portability (see scripts/run-playbook-tests.sh).
    SPECS=()
    while IFS= read -r s; do SPECS+=("$s"); done < <(git diff --cached --name-only --diff-filter=ACMRT | grep -E '^docs/v[0-9]+(\.[0-9]+)*/(pending|active|done)/.*\.md$' || true)
    ;;
  --all|all)
    SPECS=()
    while IFS= read -r s; do SPECS+=("$s"); done < <(find docs/v[0-9]* -type f -name '*.md' 2>/dev/null | grep -E '/(pending|active)/' || true)
    ;;
  --include-done|include-done)
    SPECS=()
    while IFS= read -r s; do SPECS+=("$s"); done < <(find docs/v[0-9]* -type f -name '*.md' 2>/dev/null | grep -E '/(pending|active|done)/' || true)
    ;;
  --file|file)
    # Single named spec, using the same checks as the native criterion.
    [[ -n "${2:-}" ]] || { printf "usage: %s --file <spec path>\n" "$0" >&2; exit 64; }
    [[ -f "$2" ]] || { printf "FAIL: spec not found: %s\n" "$2" >&2; exit 1; }
    SPECS=("$2")
    ;;
  *)
    printf "usage: %s [--staged|--all|--include-done|--file <path>]\n" "$0" >&2
    exit 64
    ;;
esac

if [[ ${#SPECS[@]} -eq 0 ]]; then
  ok "no spec files in scope ($MODE)"
  exit 0
fi

# Staged checks grade the index, including files removed only from the worktree.
CONTENT_DIR="$(mktemp -d)"
trap 'rm -rf "$CONTENT_DIR"' EXIT
for spec in "${SPECS[@]}"; do
  content_file="$spec"
  case "$MODE" in
    --staged|staged)
      content_file="$CONTENT_DIR/spec.md"
      if ! git show ":$spec" > "$content_file"; then
        fail "$spec — cannot read staged content"
        continue
      fi
      ;;
  esac
  [[ -f "$content_file" ]] || { fail "$spec — cannot read spec"; continue; }
  if ! command -v bun >/dev/null 2>&1; then
    fail "$spec — Bun is required for spec checks"
  elif ! bun "$SCRIPT_DIR/spec-template.ts" "$spec" "$content_file" "$MODE"; then
    FAIL=1
  fi
done

if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 SPEC TEMPLATE GATE: violations found. See dispatch/write_spec.md.\n" >&2
  exit 1
fi

ok "SPEC TEMPLATE GATE: clean (${#SPECS[@]} specs)"
exit 0
