#!/usr/bin/env bash
# audit-logging.sh — flag log-emit drift against LOGGING_STANDARD.md.
#
# Gate body: docs/gates/logging.md
# Standalone tool — not wired into make lint or pre-commit (per Captain's
# direction, gates fire pre-emptively via printed proof-blocks during
# agent work).
#
# Two severity tiers:
#   BLOCK  — exits 1, must fix:
#            - `std.debug.print(` in non-test source under src/.
#            - `console.log/debug/info/warn/error` in zombiectl/src outside tests.
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
#   --all      (default) full src/ + zombiectl/src/ scan
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

# ---------------------------------------------------------------------------
# 1. Gather files in scope.
# ---------------------------------------------------------------------------
gather_paths() {
  case "$MODE" in
    --staged)
      git diff --cached --name-only --diff-filter=ACMRT \
        | grep -E '^(src/.*\.zig$|zombiectl/src/.*\.(js|jsx|ts|tsx)$)' || true
      ;;
    --all)
      find src -type f -name '*.zig' 2>/dev/null
      find zombiectl/src -type f \( -name '*.js' -o -name '*.jsx' -o -name '*.ts' -o -name '*.tsx' \) 2>/dev/null
      ;;
  esac
}

mapfile -t FILES < <(gather_paths)
if [[ ${#FILES[@]} -eq 0 ]]; then
  ok "no source files in scope ($MODE)"
  exit 0
fi

# ---------------------------------------------------------------------------
# 2. BLOCKING: std.debug.print in non-test Zig source.
# ---------------------------------------------------------------------------
debug_print_hits=0
for f in "${FILES[@]}"; do
  [[ "$f" == *.zig ]] || continue
  # File-level test exclusions: *_test.zig, test_harness.zig,
  # *_test_harness.zig, test_helper.zig, *_test_helper.zig, anything
  # under tests/.
  [[ "$f" == *_test.zig ]] && continue
  [[ "$(basename "$f")" == test_harness.zig ]] && continue
  [[ "$f" == *_test_harness.zig ]] && continue
  [[ "$(basename "$f")" == test_helper.zig ]] && continue
  [[ "$f" == *_test_helper.zig ]] && continue
  [[ "$f" == */tests/* ]] && continue

  # Scan with awk so we can skip lines inside `test "..."` blocks
  # (inline tests in non-_test.zig files — common in some modules).
  while IFS= read -r match; do
    [[ -z "$match" ]] && continue
    fail "$f:$match — \`std.debug.print\` in non-test source (LOGGING_STANDARD §10A.L1)"
    debug_print_hits=$((debug_print_hits + 1))
  done < <(awk '
    /^test "/ { in_test = 1; next }
    /^}/ { in_test = 0; next }
    /\bstd\.debug\.print\(/ { if (!in_test) print NR }
  ' "$f")
done

# ---------------------------------------------------------------------------
# 3. BLOCKING: console.log/debug/info/warn/error in zombiectl/src non-test.
# ---------------------------------------------------------------------------
console_hits=0
for f in "${FILES[@]}"; do
  case "$f" in
    zombiectl/src/*.test.*|zombiectl/src/*.spec.*|zombiectl/src/tests/*) continue ;;
    zombiectl/src/*.js|zombiectl/src/*.jsx|zombiectl/src/*.ts|zombiectl/src/*.tsx) ;;
    *) continue ;;
  esac
  while IFS= read -r match; do
    [[ -z "$match" ]] && continue
    fail "$f:$match — \`console.*\` in non-test source (BUN_RULES §10, LOGGING_STANDARD §8)"
    console_hits=$((console_hits + 1))
  done < <(grep -nE '\bconsole\.(log|debug|info|warn|error)\(' "$f" | cut -d: -f1)
done

# ---------------------------------------------------------------------------
# 4. INFO: std.log.scoped outside src/logging/ (pre-migration to the named
#    `log` module's scoped API). The audit no longer carves out src/auth/ —
#    the named module is import-able from layer-isolated trees, so the
#    portability exception is gone.
# ---------------------------------------------------------------------------
scoped_hits=0
for f in "${FILES[@]}"; do
  [[ "$f" == *.zig ]] || continue
  case "$f" in
    src/logging/*) continue ;;
  esac
  count=$(grep -cE '\bstd\.log\.scoped\(' "$f" 2>/dev/null) || count=0
  if [[ "$count" -gt 0 ]]; then
    note "$f — $count call(s) to \`std.log.scoped\` (migrate to \`logging.scoped\` per LOGGING_STANDARD §7)"
    scoped_hits=$((scoped_hits + count))
  fi
done

# ---------------------------------------------------------------------------
# 5. INFO: err/warn logs without `error_code=` substring on the same line.
#    Heuristic — captures the common case where an err/warn line should
#    embed UZ-XXX-NNN per LOGGING_STANDARD §5.
# ---------------------------------------------------------------------------
missing_code_hits=0
for f in "${FILES[@]}"; do
  [[ "$f" == *.zig ]] || continue
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    ln="${line%%:*}"
    rest="${line#*:}"
    if ! grep -q 'error_code=' <<<"$rest"; then
      note "$f:$ln — \`std.log.{err,warn}\` without \`error_code=\` (LOGGING_STANDARD §5)"
      missing_code_hits=$((missing_code_hits + 1))
    fi
  done < <(grep -nE '\bstd\.log\.(err|warn)\b' "$f")
done

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
  printf "\n🔴 LOGGING GATE: blocking violations. See docs/gates/logging.md.\n" >&2
  exit 1
fi
[[ $INFO_COUNT -gt 0 ]] && note "$INFO_COUNT informational findings; not blocking. Use --strict to enforce."
ok "LOGGING GATE: clean (blocking layer)"
exit 0
