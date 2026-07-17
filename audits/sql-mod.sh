#!/usr/bin/env bash
# sql-mod.sh — SQL statements must live in domain sql.zig, not inline in Zig source.
#
# Dispatch façade: dispatch/write_zig.md (SQLMOD). Extracted from write_zig.sh's
# former inline `zig_sql_module_gate` so the Zig dispatch DELEGATES to a leaf
# rather than doing the work itself (the dispatch/lib.sh contract: "It never does
# the work itself — the leaf helpers in audits/ do"). This is what lets
# evals/dispatch/coverage.sh see SQLMOD as a wired dispatch_run_helper row.
#
# Rule: a *.zig file (excluding sql.zig, *_test.zig, test_fixtures_*.zig) must
# not DEFINE SQL — neither
#   - a `const <SQL-ish name> = …` (SELECT_/INSERT_/…/_SQL/_QUERY), nor
#   - a Zig multiline-string body opening with `\\WITH|SELECT|INSERT|…`.
# New files are whole-file scanned; existing files are scanned only for newly
# ADDED lines (staged diff), so pre-existing inline SQL is not retroactively
# flagged — only new SQL must land in a domain sql.zig.
#
# Modes:
#   --staged   diff-scope: staged *.zig (new files whole-scanned, existing diff-scanned)
#   --all      (default) every src/*.zig whole-scanned
#
# Exits 0 clean, 1 on any inline-SQL hit.

set -euo pipefail

MODE="${1:-${SCOPE:---all}}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

FAIL=0
fail() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()   { printf "OK:   %s\n" "$*"; }

# Kept byte-identical to the former write_zig.sh regexes so behaviour is
# unchanged by the extraction.
DEF_RE='^[[:space:]]*(pub[[:space:]]+)?const[[:space:]]+(SELECT|INSERT|UPDATE|DELETE|UPSERT|LIST|COUNT|EXISTS|[A-Z0-9_]+_(SQL|QUERY)|(SELECT|INSERT|UPDATE|DELETE|UPSERT|LIST|COUNT|EXISTS)_[A-Z0-9_]+)[[:space:]]*='
BODY_RE='^[[:space:]]*\\\\(WITH|SELECT|INSERT|UPDATE|DELETE|CREATE|ALTER|DROP)[[:space:]]'

definition_hit() { grep -nE "$DEF_RE" "$1" | head -1 || true; }
body_hit()       { grep -nE "$BODY_RE" "$1" | head -1 || true; }
added_hit() {
  git diff --cached -U0 -- "$1" \
    | awk '/^\+[^+]/ { sub(/^\+/, ""); print }' \
    | grep -nE "$DEF_RE|$BODY_RE" | head -1 || true
}

case "$MODE" in
  --staged|staged)
    mapfile -t FILES < <(git diff --cached --name-only --diff-filter=ACMRT -- '*.zig' || true)
    STAGED=1 ;;
  --all|all)
    mapfile -t FILES < <(find src -type f -name '*.zig' 2>/dev/null || true)
    STAGED=0 ;;
  *)
    printf "usage: %s [--staged|--all]\n" "$0" >&2
    exit 64 ;;
esac

if [[ ${#FILES[@]} -eq 0 ]]; then
  ok "no zig source files in scope ($MODE)"
  exit 0
fi

scanned=0
for f in "${FILES[@]}"; do
  [[ -n "$f" ]] || continue
  case "$(basename "$f")" in
    sql.zig|*_test.zig|test_fixtures_*.zig) continue ;;
  esac
  [[ -f "$f" ]] || continue
  scanned=$((scanned + 1))

  is_new=0
  if [[ "$STAGED" -eq 1 ]]; then
    status="$(git diff --cached --name-status -- "$f" | awk 'NR==1 {print $1}')"
    if [[ "$status" == A* ]]; then
      is_new=1
    elif ! git ls-files --error-unmatch "$f" >/dev/null 2>&1; then
      is_new=1
    fi
  else
    is_new=1   # --all: no diff context — whole-file scan
  fi

  if [[ "$is_new" -eq 1 ]]; then
    line="$(definition_hit "$f")"
    [[ -n "$line" ]] || line="$(body_hit "$f")"
  else
    line="$(added_hit "$f")"
  fi

  if [[ -n "$line" ]]; then
    fail "$f adds inline SQL ($line); move statements to domain sql.zig"
  fi
done

if [[ "$FAIL" -ne 0 ]]; then
  printf "\n🔴 SQLMOD: inline SQL found — move statements to a domain sql.zig. See dispatch/write_zig.md.\n" >&2
  exit 1
fi
ok "SQLMOD: clean ($scanned file(s) scanned, no new inline SQL)"
exit 0
