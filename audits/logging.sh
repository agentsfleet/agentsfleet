#!/usr/bin/env bash
# logging.sh — flag log-emit drift against LOGGING_STANDARD.md.
#
# Dispatch façade: dispatch/write_any.md (Logging Gate)
# Fires in: CONFORM (via `make harness-verify` in `agentsfleet`).
#
# TECHNICAL DEBT (acknowledged on migration to dotfiles, 2026-05-11):
# The script hard-codes:
#   - agentsfleet scope-prefix format from LOGGING_STANDARD §7 (the
#     `log.scoped(...)` API path under `src/logging/`)
#   - `UZ-XXX-NNN` as the `error_code=` substring per LOGGING_STANDARD §5
#   - `src/` + `agentsfleet/src/` as the scan roots
# A parameterised version that reads prefix + scope-api path + scan
# roots from a per-project config is the right long-term shape; not
# done yet. Today, this script lives in dotfiles so the gate body and
# the audit binary stay co-located.
#
# Two severity tiers:
#   BLOCK  — exits 1, must fix:
#            - `std.debug.print(` in non-test source under src/.
#            - `console.log/debug/info/warn/error` in agentsfleet/src outside tests.
#   INFO   — surfaced for reviewer/agent attention, doesn't block:
#            - `std.log.scoped(...)` outside `src/logging/` (LOGGING_STANDARD §7
#              says only the named-module `log.scoped` API should be used;
#              today's callers are pre-migration).
#            - `std.log.{err,warn,info,debug}` calls (positional fmt format) — the
#              old API. Migration to `log.<level>("event", .{...})` pending.
#            - `err`/`warn` log calls without an `error_code=` substring nearby.
#
# Modes:
#   --staged   diff-scope: only files in `git diff --cached`
#   --all      (default) full src/ + agentsfleet/src/ scan
#   --strict   promote every INFO finding to BLOCK (post-migration use)
#
# Exits 0 clean, 1 on BLOCK findings.

set -euo pipefail

MODE="--all"
STRICT=0
for arg in "$@"; do
  case "$arg" in
    --staged|staged) MODE="--staged" ;;
    --all|all)       MODE="--all" ;;
    --strict)        STRICT=1 ;;
    -h|--help)
      printf "usage: %s [--staged|--all] [--strict]\n" "$0"
      exit 0
      ;;
    *) printf "unknown arg: %s\n" "$arg" >&2; exit 64 ;;
  esac
done

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

FAIL=0
INFO_COUNT=0
fail() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()   { printf "OK:   %s\n" "$*"; }
note() { printf "INFO: %s\n" "$*"; INFO_COUNT=$((INFO_COUNT + 1)); }

# Test-file carve-out — skip Zig test sources from every detection
# pass. Test code legitimately exercises log emits (deliberate err
# paths, logger smoke tests) and uses raw std.log to keep tests
# self-contained; the migration discipline applies to production
# source. Returns 0 (skip) for test paths, 1 (scan) otherwise.
is_test_zig() {
  case "$1" in
    *_test.zig) return 0 ;;
    *_test_harness.zig) return 0 ;;
    *_test_helper.zig) return 0 ;;
    */tests/*) return 0 ;;
  esac
  case "$(basename "$1")" in
    test_harness.zig) return 0 ;;
    test_helper.zig) return 0 ;;
  esac
  return 1
}

# ---------------------------------------------------------------------------
# 1. Gather files in scope.
# ---------------------------------------------------------------------------
gather_paths() {
  case "$MODE" in
    --staged)
      git diff --cached --name-only --diff-filter=ACMRT \
        | grep -E '^(src/.*\.zig$|agentsfleet/src/.*\.(js|jsx|ts|tsx)$)' || true
      ;;
    --all)
      find src -type f -name '*.zig' 2>/dev/null
      find agentsfleet/src -type f \( -name '*.js' -o -name '*.jsx' -o -name '*.ts' -o -name '*.tsx' \) 2>/dev/null
      ;;
  esac
}

mapfile -t FILES < <(gather_paths)
if [[ ${#FILES[@]} -eq 0 ]]; then
  ok "no source files in scope ($MODE)"
  exit 0
fi

# Build the in-scope file subsets once. The per-section loops below
# previously ran 3–4 forks × N files; M70 perf pass batches them into
# single awk/grep passes.
zig_nontest=()
js_nontest=()
for f in "${FILES[@]}"; do
  if [[ "$f" == *.zig ]] && ! is_test_zig "$f"; then
    zig_nontest+=("$f")
  fi
  case "$f" in
    agentsfleet/src/*.test.*|agentsfleet/src/*.spec.*|agentsfleet/src/tests/*) ;;
    agentsfleet/src/*.js|agentsfleet/src/*.jsx|agentsfleet/src/*.ts|agentsfleet/src/*.tsx)
      js_nontest+=("$f")
      ;;
  esac
done

# ---------------------------------------------------------------------------
# 2. BLOCKING: std.debug.print in non-test Zig source.
# ---------------------------------------------------------------------------
debug_print_hits=0
if [[ ${#zig_nontest[@]} -gt 0 ]]; then
  # Single awk across every non-test .zig file; FNR == 1 resets the
  # `test "..."` block tracker per file. Skips lines inside inline tests.
  while IFS= read -r match; do
    [[ -z "$match" ]] && continue
    f="${match%%:*}"
    ln="${match#*:}"
    fail "$f:$ln — \`std.debug.print\` in non-test source (LOGGING_STANDARD §10A.L1)"
    debug_print_hits=$((debug_print_hits + 1))
  done < <(awk '
    FNR == 1 { in_test = 0 }
    /^test "/ { in_test = 1; next }
    /^}/ { in_test = 0; next }
    /(^|[^A-Za-z0-9_])std\.debug\.print\(/ { if (!in_test) printf "%s:%d\n", FILENAME, FNR }
  ' "${zig_nontest[@]}")
fi

# ---------------------------------------------------------------------------
# 3. BLOCKING: console.log/debug/info/warn/error in agentsfleet/src non-test.
# ---------------------------------------------------------------------------
console_hits=0
if [[ ${#js_nontest[@]} -gt 0 ]]; then
  while IFS= read -r match; do
    [[ -z "$match" ]] && continue
    f="${match%%:*}"
    rest="${match#*:}"
    ln="${rest%%:*}"
    fail "$f:$ln — \`console.*\` in non-test source (write_ts_adhere_bun.md §10, LOGGING_STANDARD §8)"
    console_hits=$((console_hits + 1))
  done < <(grep -nHE '\bconsole\.(log|debug|info|warn|error)\(' "${js_nontest[@]}" 2>/dev/null || true)
fi

# ---------------------------------------------------------------------------
# 4. INFO: std.log.scoped outside src/logging/ (pre-migration to the named
#    `log` module's scoped API). The audit no longer carves out src/auth/ —
#    the named module is import-able from layer-isolated trees, so the
#    portability exception is gone.
# ---------------------------------------------------------------------------
scoped_hits=0
scoped_eligible=()
for f in "${zig_nontest[@]}"; do
  case "$f" in
    src/logging/*) continue ;;
  esac
  scoped_eligible+=("$f")
done
if [[ ${#scoped_eligible[@]} -gt 0 ]]; then
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    count="${line%% *}"
    f="${line#* }"
    note "$f — $count call(s) to \`std.log.scoped\` (migrate to \`logging.scoped\` per LOGGING_STANDARD §7)"
    scoped_hits=$((scoped_hits + count))
  done < <(grep -cHE '\bstd\.log\.scoped\(' "${scoped_eligible[@]}" 2>/dev/null \
    | awk -F: '$2 > 0 { print $2, $1 }')
fi

# ---------------------------------------------------------------------------
# 5. INFO: err/warn logs without `error_code=` substring on the same line.
#    Heuristic — captures the common case where an err/warn line should
#    embed UZ-XXX-NNN per LOGGING_STANDARD §5.
# ---------------------------------------------------------------------------
missing_code_hits=0
if [[ ${#zig_nontest[@]} -gt 0 ]]; then
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    [[ "$line" == *error_code=* ]] && continue
    f="${line%%:*}"
    rest="${line#*:}"
    ln="${rest%%:*}"
    note "$f:$ln — \`std.log.{err,warn}\` without \`error_code=\` (LOGGING_STANDARD §5)"
    missing_code_hits=$((missing_code_hits + 1))
  done < <(grep -nHE '\bstd\.log\.(err|warn)\b' "${zig_nontest[@]}" 2>/dev/null || true)
fi

# ---------------------------------------------------------------------------
# 6. Promote INFO to BLOCK in --strict mode.
# ---------------------------------------------------------------------------
if [[ $STRICT -eq 1 && $INFO_COUNT -gt 0 ]]; then
  fail "--strict: $INFO_COUNT informational findings promoted to blocking"
fi

# ---------------------------------------------------------------------------
# 7. Verdict.
# ---------------------------------------------------------------------------
ok "scanned ${#FILES[@]} files; std.debug.print=$debug_print_hits console.*=$console_hits std.log.scoped=$scoped_hits missing-error_code=$missing_code_hits"
if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 LOGGING GATE: blocking violations. See dispatch/write_any.md (Logging Gate).\n" >&2
  exit 1
fi
[[ $INFO_COUNT -gt 0 ]] && note "$INFO_COUNT informational findings; not blocking. Use --strict to enforce."
ok "LOGGING GATE: clean (blocking layer)"
exit 0
