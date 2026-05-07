#!/usr/bin/env bash
# audit-error-codes.sh — orphan + dead code detection against the canonical
# error registry at src/errors/error_registry.zig, plus a raw-literal check
# enforcing that UZ-<CAT>-<NNN> strings only appear in registry/allowlist
# files (every other call site must reference the registry symbol).
#
# Gate body: docs/gates/error-registry.md
# Fires in: make lint (after audit-spec-template, before audit-logging).
#
# Definitions:
#   - DECLARED  — every UZ-<CAT>-<NNN> string literal in error_registry.zig
#   - USED      — every UZ-<CAT>-<NNN> reference in src/**/*.zig (excluding
#                  the registry itself) plus zombiectl/src/** (any extension)
#   - ORPHAN    — USED but not DECLARED  (BLOCKING)
#   - DEAD      — DECLARED but not USED  (INFORMATIONAL)
#   - RAW LEAK  — UZ-<CAT>-<NNN> string literal in a source file outside
#                  the registry/allowlist (BLOCKING — RULE UFS at the error
#                  surface; non-allowlisted call sites must use a registry
#                  symbol, never duplicate the literal).
#
# Raw-literal allowlist (files where UZ-* literals are legitimate):
#   - src/errors/**                         (canonical registry + entry tables)
#   - src/executor/client_errors.zig        (executor crate's own mirror file
#                                            — see file header for build-module
#                                            isolation rationale; deliberate
#                                            duplication, all other executor
#                                            sources import from there)
#   - **/*_test.zig                         (tests)
#   - src/zbench_fixtures.zig               (benchmark fixture seeds)
#   - src/http/test_harness.zig             (HTTP test harness)
#   - lines starting with `//`              (comments)
#   - lines annotated `// audit-error-codes: intentional-fake`
#
# Tests INCLUDED for orphan detection — same code-registry rules apply to
# test code. A test referencing a fake code (e.g. UZ-FAKE-999) for
# negative-path testing must annotate the line:
# `// audit-error-codes: intentional-fake` and the audit will skip it.
#
# Modes:
#   --staged   diff-scope: only files in `git diff --cached` are checked
#              for new orphan refs (registry checked unconditionally)
#   --all      (default) full repo scan
#
# Exits 0 clean, 1 on orphans or raw leaks (DEAD findings never block).

set -euo pipefail

MODE="${1:-${SCOPE:-all}}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

REGISTRY="src/errors/error_registry.zig"
[[ -f "$REGISTRY" ]] || { printf "FAIL: registry missing: %s\n" "$REGISTRY" >&2; exit 1; }

FAIL=0
fail() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()   { printf "OK:   %s\n" "$*"; }
note() { printf "NOTE: %s\n" "$*"; }

# ---------------------------------------------------------------------------
# 1. Extract DECLARED codes from the registry.
#    Pattern: literal "UZ-<CAT>-<NNN>" inside the registry file.
# ---------------------------------------------------------------------------
declared_codes=$(grep -oE 'UZ-[A-Z]+-[0-9]{3,}\b' "$REGISTRY" | sort -u)
if [[ -z "$declared_codes" ]]; then
  fail "no codes declared in $REGISTRY (registry empty?)"
  exit 1
fi
declared_count=$(printf '%s\n' "$declared_codes" | wc -l | tr -d ' ')

# ---------------------------------------------------------------------------
# 2. Determine USED codes.
#    Source set: src/**/*.zig minus *_test.zig minus the registry itself,
#    plus zombiectl/src/**.
# ---------------------------------------------------------------------------
gather_used_paths() {
  case "$MODE" in
    --staged|staged)
      git diff --cached --name-only --diff-filter=ACMRT \
        | grep -E '^(src/|zombiectl/src/)' \
        | grep -vE '^src/errors/error_registry\.zig$' || true
      ;;
    --all|all)
      {
        find src -type f -name '*.zig' 2>/dev/null \
          | grep -vE '^src/errors/error_registry\.zig$'
        find zombiectl/src -type f 2>/dev/null
      }
      ;;
    *)
      printf "usage: %s [--staged|--all]\n" "$0" >&2
      exit 64
      ;;
  esac
}

mapfile -t USED_PATHS < <(gather_used_paths)
if [[ ${#USED_PATHS[@]} -eq 0 ]]; then
  ok "no source files in scope ($MODE)"
  exit 0
fi

used_codes=$(awk '
  # Reset per-file so a trailing intentional-fake marker in the previous
  # file cannot carry skip_next=1 into the first line of the next file.
  FNR == 1 { skip_next = 0 }
  # Skip the line carrying the marker AND the next non-blank line (the
  # marker sits on the comment line above the code line in Zig style).
  /audit-error-codes: intentional-fake/ { skip_next = 1; next }
  /^[[:space:]]*$/ { next }
  skip_next { skip_next = 0; next }
  {
    line = $0;
    while (match(line, /UZ-[A-Z]+-[0-9][0-9][0-9][0-9]*/)) {
      m = substr(line, RSTART, RLENGTH);
      # Reject codes whose digit run is followed by a word char (rejects
      # placeholder forms like UZ-INTERNAL-00X that grep would truncate).
      tail_pos = RSTART + RLENGTH;
      if (tail_pos <= length(line)) {
        tail_char = substr(line, tail_pos, 1);
        if (tail_char ~ /[A-Za-z0-9_]/) {
          line = substr(line, tail_pos + 1);
          continue;
        }
      }
      print m;
      line = substr(line, tail_pos);
    }
  }
' "${USED_PATHS[@]}" 2>/dev/null | sort -u || true)
used_count=0
[[ -n "$used_codes" ]] && used_count=$(printf '%s\n' "$used_codes" | wc -l | tr -d ' ')

# ---------------------------------------------------------------------------
# 3. Compute orphans (USED − DECLARED). Blocking.
# ---------------------------------------------------------------------------
orphans=$(comm -23 <(printf '%s\n' "$used_codes") <(printf '%s\n' "$declared_codes") | grep -v '^$' || true)
if [[ -n "$orphans" ]]; then
  fail "orphan codes (used but not declared in $REGISTRY):"
  while IFS= read -r code; do
    [[ -z "$code" ]] && continue
    printf "        %s  refs:\n" "$code" >&2
    grep -lE "$code" "${USED_PATHS[@]}" 2>/dev/null \
      | head -3 \
      | sed 's/^/          - /' >&2 || true
  done <<<"$orphans"
fi

# ---------------------------------------------------------------------------
# 4. Compute dead codes (DECLARED − USED). Informational only.
#    --staged mode skips this (full set isn't computed against partial diff).
# ---------------------------------------------------------------------------
if [[ "$MODE" = "--all" || "$MODE" = "all" ]]; then
  dead=$(comm -13 <(printf '%s\n' "$used_codes") <(printf '%s\n' "$declared_codes") | grep -v '^$' || true)
  if [[ -n "$dead" ]]; then
    dead_count=$(printf '%s\n' "$dead" | wc -l | tr -d ' ')
    note "dead codes (declared but unreferenced — informational, $dead_count total):"
    printf '%s\n' "$dead" | head -10 | sed 's/^/        /'
    [[ "$dead_count" -gt 10 ]] && printf "        ... and %d more\n" "$((dead_count - 10))"
  fi
fi

# ---------------------------------------------------------------------------
# 4.5. Raw-literal check.
#      Any "UZ-…" string literal outside the allowlist (and not inside a
#      `//` comment or annotated with `audit-error-codes: intentional-fake`)
#      is a violation: the call site must reference a registry symbol.
#
#      Multi-segment codes (e.g. `UZ-OBS-OTEL-LOG-001`) are caught too,
#      even though the orphan/dead detection above uses a stricter
#       2-segment pattern. We don't want a "UZ-X-Y-NNN" code reintroduced
#      via a local const that the orphan pass silently ignores.
# ---------------------------------------------------------------------------
raw_leaks=$(awk '
  FNR == 1 { skip_next = 0 }
  # Skip the line carrying the marker AND the next non-blank line.
  /audit-error-codes: intentional-fake/ { skip_next = 1; next }
  /^[[:space:]]*\/\// { next }
  /^[[:space:]]*$/ { next }
  skip_next { skip_next = 0; next }
  # Match any "UZ-XXX(-YYY)*-NNN" string literal.
  /"UZ-[A-Z][A-Z0-9-]*-[0-9]+"/ {
    print FILENAME ":" FNR ":" $0;
  }
' $(printf '%s\n' "${USED_PATHS[@]}" | grep -vE \
    '^src/errors/|/_?test_harness\.zig$|_test\.zig$|^src/executor/client_errors\.zig$|^src/zbench_fixtures\.zig$|^zombiectl/src/constants/error-codes\.js$' \
    || true) 2>/dev/null || true)

if [[ -n "$raw_leaks" ]]; then
  fail "raw UZ-* literals found outside the registry allowlist (must reference a registry symbol):"
  printf '%s\n' "$raw_leaks" | head -20 | sed 's/^/        /' >&2
  raw_count=$(printf '%s\n' "$raw_leaks" | wc -l | tr -d ' ')
  if [[ "$raw_count" -gt 20 ]]; then
    printf "        ... and %d more\n" "$((raw_count - 20))" >&2
  fi
fi

# ---------------------------------------------------------------------------
# 5. Verdict.
# ---------------------------------------------------------------------------
ok "registry: $declared_count declared, $used_count used"
if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 ERROR REGISTRY GATE: violations found. Reference the registry symbol (e.g. \`error_codes.ERR_X\` or \`client_errors.ERR_X\` for the executor crate) instead of duplicating the literal.\n" >&2
  exit 1
fi
ok "ERROR REGISTRY GATE: clean"
exit 0
