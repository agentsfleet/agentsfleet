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
#   - `src/` + `agentsfleet/src/` as the Zig and TypeScript scan roots
# A parameterised version that reads prefix + scope-api path + scan
# roots from a per-project config is the right long-term shape; not
# done yet. Today, this script lives in dotfiles so the gate body and
# the audit binary stay co-located.
#
# Two severity tiers:
#   BLOCK  — exits 1, must fix:
#            - `std.debug.print(` in non-test source under src/.
#            - `println!` / `eprintln!` / `dbg!`, missing `event`, or positional
#              tracing formatting in runtime Rust source. Direct stream writes
#              may carry `// logging: <reason>` on or immediately above the
#              emit when stdout or stderr is the intended program interface.
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
#   --all      (default) full src/ + every tracked/unignored *.rs +
#              agentsfleet/src/ scan
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

is_non_runtime_rust_path() {
  case "$1" in
    build.rs|*/build.rs|tests/*|*/tests/*|benches/*|*/benches/*|examples/*|*/examples/*) return 0 ;;
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
        | grep -E '(^src/.*\.zig$|\.rs$|^agentsfleet/src/.*\.(js|jsx|ts|tsx)$)' || true
      ;;
    --all)
      find src -type f -name '*.zig' 2>/dev/null || true
      while IFS= read -r path; do
        [[ -f "$path" ]] && printf '%s\n' "$path"
      done < <(git ls-files --cached --others --exclude-standard -- '*.rs')
      find agentsfleet/src -type f \( -name '*.js' -o -name '*.jsx' -o -name '*.ts' -o -name '*.tsx' \) 2>/dev/null || true
      ;;
  esac
}

# `while read` rather than mapfile: mapfile is bash 4+ and macOS ships 3.2 —
# the portability rule scripts/run-playbook-tests.sh already records.
FILES=()
while IFS= read -r p; do FILES+=("$p"); done < <(gather_paths)
if [[ ${#FILES[@]} -eq 0 ]]; then
  ok "no source files in scope ($MODE)"
  exit 0
fi

# Build the in-scope file subsets once. The per-section loops below
# previously ran 3–4 forks × N files; M70 perf pass batches them into
# single awk/grep passes.
zig_nontest=()
rust_nontest=()
rust_candidates=()
js_nontest=()
for f in "${FILES[@]}"; do
  if [[ "$f" == *.zig ]] && ! is_test_zig "$f"; then
    zig_nontest+=("$f")
  fi
  if [[ "$f" == *.rs ]] && ! is_non_runtime_rust_path "$f"; then
    rust_nontest+=("$f")
  fi
  case "$f" in
    agentsfleet/src/*.test.*|agentsfleet/src/*.spec.*|agentsfleet/src/tests/*) ;;
    agentsfleet/src/*.js|agentsfleet/src/*.jsx|agentsfleet/src/*.ts|agentsfleet/src/*.tsx)
      js_nontest+=("$f")
      ;;
  esac
done

if [[ ${#rust_nontest[@]} -gt 0 ]]; then
  rust_candidates=()
  while IFS= read -r p; do rust_candidates+=("$p"); done \
    < <(grep -lE '(^|[^[:alnum:]_])(println|eprintln|dbg)!|tracing::(error|warn|info|debug|trace)!' "${rust_nontest[@]}" 2>/dev/null || true)
fi

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
    function comment_text(text, output, cursor, char, escaped, quoted) {
      escaped = 0
      quoted = 0
      for (cursor = 1; cursor <= length(text); cursor++) {
        char = substr(text, cursor, 1)
        if (quoted) {
          if (escaped) escaped = 0
          else if (char == "\\") escaped = 1
          else if (char == "\"") quoted = 0
          continue
        }
        if (char == "\"") {
          quoted = 1
          continue
        }
        if (char == "/" && substr(text, cursor + 1, 1) == "/")
          return substr(text, cursor + 2)
      }
      return ""
    }
    function has_logging_reason(text, comment) {
      if (text ~ /^[[:space:]]*\\\\/) return 0
      comment = comment_text(text)
      return comment ~ /^[[:space:]]*logging:[[:space:]]*[^[:space:]]/
    }
    FNR == 1 { in_test = 0; previous_annotation = 0 }
    {
      annotated = has_logging_reason($0) || previous_annotation
      if ($0 ~ /^test "/) in_test = 1
      else if ($0 ~ /^}/) in_test = 0
      else if ($0 ~ /(^|[^A-Za-z0-9_])std\.debug\.print\(/ && !in_test && !annotated)
        printf "%s:%d\n", FILENAME, FNR
      previous_annotation = has_logging_reason($0)
    }
  ' "${zig_nontest[@]}")
fi

# ---------------------------------------------------------------------------
# 3. BLOCKING: unstructured or event-less tracing in non-test Rust source.
#    The awk tracker excludes complete items annotated with #[cfg(test)].
# ---------------------------------------------------------------------------
rust_direct_hits=0
rust_missing_event_hits=0
rust_positional_hits=0
if [[ ${#rust_candidates[@]} -gt 0 ]]; then
  while IFS='|' read -r kind f ln; do
    [[ -z "$kind" ]] && continue
    case "$kind" in
      direct)
        fail "$f:$ln — direct Rust diagnostic macro in non-test source (LOGGING_STANDARD §8A)"
        rust_direct_hits=$((rust_direct_hits + 1))
        ;;
      event)
        fail "$f:$ln — \`tracing\` emit without \`event = ...\` (LOGGING_STANDARD §8A)"
        rust_missing_event_hits=$((rust_missing_event_hits + 1))
        ;;
      positional)
        fail "$f:$ln — positional formatting in \`tracing\` emit (LOGGING_STANDARD §8A)"
        rust_positional_hits=$((rust_positional_hits + 1))
        ;;
    esac
  done < <(awk '
    function count_char(text, char, copy, count) {
      copy = text
      count = gsub(char, "", copy)
      return count
    }
    function code_only(text, output, cursor, char, escaped, quoted) {
      output = ""
      escaped = 0
      quoted = 0
      for (cursor = 1; cursor <= length(text); cursor++) {
        char = substr(text, cursor, 1)
        if (quoted) {
          if (escaped) escaped = 0
          else if (char == "\\") escaped = 1
          else if (char == "\"") quoted = 0
          continue
        }
        if (char == "\"") {
          quoted = 1
          continue
        }
        if (char == "/" && substr(text, cursor + 1, 1) == "/") break
        output = output char
      }
      return output
    }
    function without_comment(text, output, cursor, char, escaped, quoted) {
      output = ""
      escaped = 0
      quoted = 0
      for (cursor = 1; cursor <= length(text); cursor++) {
        char = substr(text, cursor, 1)
        if (quoted) {
          output = output char
          if (escaped) escaped = 0
          else if (char == "\\") escaped = 1
          else if (char == "\"") quoted = 0
          continue
        }
        if (char == "\"") {
          quoted = 1
          output = output char
          continue
        }
        if (char == "/" && substr(text, cursor + 1, 1) == "/") break
        output = output char
      }
      return output
    }
    function comment_text(text, cursor, char, escaped, quoted) {
      escaped = 0
      quoted = 0
      for (cursor = 1; cursor <= length(text); cursor++) {
        char = substr(text, cursor, 1)
        if (quoted) {
          if (escaped) escaped = 0
          else if (char == "\\") escaped = 1
          else if (char == "\"") quoted = 0
          continue
        }
        if (char == "\"") {
          quoted = 1
          continue
        }
        if (char == "/" && substr(text, cursor + 1, 1) == "/")
          return substr(text, cursor + 2)
      }
      return ""
    }
    function has_logging_reason(text, comment) {
      comment = comment_text(text)
      return comment ~ /^[[:space:]]*logging:[[:space:]]*[^[:space:]]/
    }
    function check_emit() {
      if (emit_code !~ /event[[:space:]]*=/ && emit_code !~ /[(,][[:space:]]*event[[:space:]]*[,)]/)
        printf "event|%s|%d\n", FILENAME, emit_line
      if (emit_source ~ /"[^"\n]*\{[^"\n]*\}[^"\n]*"/)
        printf "positional|%s|%d\n", FILENAME, emit_line
      emit_code = ""
      emit_source = ""
      in_emit = 0
    }
    FNR == 1 {
      depth = 0
      test_depth = 0
      cfg_test = 0
      in_emit = 0
      emit_parens = 0
      emit_code = ""
      emit_source = ""
      previous_annotation = 0
    }
    {
      current_annotation = has_logging_reason($0)
      code = code_only($0)
      if (!in_emit && code ~ /^[[:space:]]*$/) {
        previous_annotation = current_annotation
        next
      }
      opens = count_char(code, "\\{")
      closes = count_char(code, "\\}")
      if (test_depth > 0) {
        depth += opens - closes
        if (depth < test_depth) test_depth = 0
        previous_annotation = current_annotation
        next
      }
      if (code ~ /^[[:space:]]*#\[cfg\(test\)\]/) {
        if (opens > 0) {
          test_depth = depth + 1
          depth += opens - closes
          if (depth < test_depth) test_depth = 0
        } else cfg_test = 1
        previous_annotation = current_annotation
        next
      }
      if (cfg_test) {
        if (opens > 0) {
          test_depth = depth + 1
          depth += opens - closes
          if (depth < test_depth) test_depth = 0
          cfg_test = 0
        } else if (code ~ /;/) cfg_test = 0
        previous_annotation = current_annotation
        next
      }
      if (code ~ /(^|[^[:alnum:]_])(println|eprintln|dbg)!/ && !current_annotation && !previous_annotation)
        printf "direct|%s|%d\n", FILENAME, FNR
      if (!in_emit && match(code, /tracing::(error|warn|info|debug|trace)!/)) {
        in_emit = 1
        emit_line = FNR
        emit_code = code
        emit_source = without_comment($0)
        macro = substr(code, RSTART)
        emit_parens = count_char(macro, "\\(") - count_char(macro, "\\)")
      } else if (in_emit) {
        emit_code = emit_code " " code
        emit_source = emit_source " " without_comment($0)
        emit_parens += count_char(code, "\\(") - count_char(code, "\\)")
      }
      if (in_emit && emit_parens <= 0) check_emit()
      depth += opens - closes
      previous_annotation = current_annotation
    }
  ' "${rust_candidates[@]}")
fi

# ---------------------------------------------------------------------------
# 4. BLOCKING: console.log/debug/info/warn/error in agentsfleet/src non-test.
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
# 5. INFO: std.log.scoped outside src/logging/ (pre-migration to the named
#    `log` module's scoped API). The audit no longer carves out src/auth/ —
#    the named module is import-able from layer-isolated trees, so the
#    portability exception is gone.
# ---------------------------------------------------------------------------
scoped_hits=0
scoped_eligible=()
# Guarded like every sibling loop: bash 3.2 under `set -u` treats "${empty[@]}"
# as unbound (fixed only in 4.4), so an unguarded expansion aborts the audit in
# any repository with no Zig sources. macOS ships 3.2.
if [[ ${#zig_nontest[@]} -gt 0 ]]; then
  for f in "${zig_nontest[@]}"; do
    case "$f" in
      src/logging/*) continue ;;
    esac
    scoped_eligible+=("$f")
  done
fi
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
# 6. INFO: err/warn logs without `error_code=` substring on the same line.
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
# 7. Promote INFO to BLOCK in --strict mode.
# ---------------------------------------------------------------------------
if [[ $STRICT -eq 1 && $INFO_COUNT -gt 0 ]]; then
  fail "--strict: $INFO_COUNT informational findings promoted to blocking"
fi

# ---------------------------------------------------------------------------
# 8. Verdict.
# ---------------------------------------------------------------------------
ok "scanned ${#FILES[@]} files; std.debug.print=$debug_print_hits rust-direct=$rust_direct_hits rust-missing-event=$rust_missing_event_hits rust-positional=$rust_positional_hits console.*=$console_hits std.log.scoped=$scoped_hits missing-error_code=$missing_code_hits"
if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 LOGGING GATE: blocking violations. See dispatch/write_any.md (Logging Gate).\n" >&2
  printf "Intentional direct Zig or Rust stream output requires '// logging: <reason>' on or immediately above the emit; the reason must be non-empty.\n" >&2
  exit 1
fi
[[ $INFO_COUNT -gt 0 ]] && note "$INFO_COUNT informational findings; not blocking. Use --strict to enforce."
ok "LOGGING GATE: clean (blocking layer)"
exit 0
