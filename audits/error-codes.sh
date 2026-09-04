#!/usr/bin/env bash
# error-codes.sh — orphan + dead code detection against a repository's canonical
# error registry, plus a raw-literal check enforcing that <PREFIX>-<CAT>-<NNN>
# strings only appear in registry/allowlist files (every other call site must
# reference the registry symbol).
#
# Dispatch façade: dispatch/write_any.md (Error Registry Gate)
# Fires in: CONFORM (via the repository's declared `conform` command).
#
# WHAT IS CONFIGURED, AND WHY IT IS NOT A PATH IN THIS FILE.
# This script used to hard-code one repository's registry path, source globs and
# allowlist. That repository retired the runtime those paths named, and the gate
# then pointed at nothing — the same silent-green failure the UFS parity check
# was fixed for. Every one of them is now resolved at run time, and the file
# names no repository's layout:
#
#   ORLY_ERROR_PREFIX     code prefix                (default: UZ)
#   ORLY_ERROR_REGISTRY   registry file(s), a glob   (default: autodetected)
#   ORLY_ERROR_SOURCES    where codes may be USED    (default: by runtime)
#   ORLY_ERROR_ALLOWLIST  extra raw-literal paths    (regex, appended)
#
# AUTODETECTION picks the runtime that actually declares codes:
#   Rust — a file declaring `ErrorCode::declare("<PREFIX>-…")`; sources are the
#          crate tree's `*.rs`.
#   Zig  — a file named `error_registry.zig`; sources are `src/**/*.zig`.
# A repository with neither SKIPS with a reason and exits 0. It does not fail,
# and it does not pass by scanning nothing and calling that clean.
#
# Definitions:
#   - DECLARED  — every <PREFIX>-<CAT>-<NNN> literal in the registry
#   - USED      — every such reference in the source set, minus the registry
#   - ORPHAN    — USED but not DECLARED  (BLOCKING)
#   - DEAD      — DECLARED but not USED  (INFORMATIONAL)
#   - RAW LEAK  — a code literal in a source file outside the registry and the
#                 allowlist (BLOCKING — RULE UFS at the error surface)
#
# Raw-literal allowlist: the registry files themselves, anything test-shaped
# (`*_test.zig`, `*_test.rs`, a `tests/` directory, a test harness), plus
# whatever `ORLY_ERROR_ALLOWLIST` adds. Lines starting `//` are comments, and a
# line annotated `// audit-error-codes: intentional-fake` skips itself and the
# line below it — which is how a negative test names a code that must not exist.
#
# Modes:
#   --staged   diff-scope: only files in `git diff --cached` are checked
#              for new orphan refs (registry checked unconditionally)
#   --all      (default) full repo scan
#
# Exits 0 clean or skipped, 1 on orphans or raw leaks (DEAD never blocks).

set -euo pipefail

MODE="${1:-${SCOPE:-all}}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

PREFIX="${ORLY_ERROR_PREFIX:-UZ}"
CODE_RE="${PREFIX}-[A-Z][A-Z0-9]*-[0-9]{3,}"

# Registry files, then the runtime whose sources may reference them. Resolved
# together: the source set follows from what declares the codes, so a repository
# cannot end up scanning one runtime's files for another runtime's registry.
REGISTRY_PATHS=()
RUNTIME=""
if [[ -n "${ORLY_ERROR_REGISTRY:-}" ]]; then
  while IFS= read -r f; do [[ -n "$f" ]] && REGISTRY_PATHS+=("$f"); done < <(
    git ls-files -- $ORLY_ERROR_REGISTRY 2>/dev/null || true)
  [[ ${#REGISTRY_PATHS[@]} -gt 0 ]] || {
    printf "FAIL: ORLY_ERROR_REGISTRY matched no tracked file: %s\n" "$ORLY_ERROR_REGISTRY" >&2
    exit 1
  }
  case "${REGISTRY_PATHS[0]}" in *.rs) RUNTIME=rust ;; *.zig) RUNTIME=zig ;; *) RUNTIME=other ;; esac
else
  while IFS= read -r f; do [[ -n "$f" ]] && REGISTRY_PATHS+=("$f"); done < <(
    git grep -l -E "ErrorCode::declare\(\"${PREFIX}-" -- '*.rs' 2>/dev/null \
      | grep -vE "_tests?\.rs$|(^|/)tests?/|(^|/)tests?\.rs$" | sort || true)
  if [[ ${#REGISTRY_PATHS[@]} -gt 0 ]]; then
    RUNTIME=rust
  else
    while IFS= read -r f; do [[ -n "$f" ]] && REGISTRY_PATHS+=("$f"); done < <(
      git ls-files -- '*error_registry.zig' 2>/dev/null | sort || true)
    [[ ${#REGISTRY_PATHS[@]} -gt 0 ]] && RUNTIME=zig
  fi
fi

if [[ ${#REGISTRY_PATHS[@]} -eq 0 ]]; then
  printf "OK:   audit-error-codes: skipped — no %s-* registry found, so this repository has no source of truth to compare against\n" "$PREFIX"
  exit 0
fi

# Where a code may legitimately be REFERENCED.
if [[ -n "${ORLY_ERROR_SOURCES:-}" ]]; then
  SOURCE_GLOB="$ORLY_ERROR_SOURCES"
elif [[ "$RUNTIME" = "rust" ]]; then
  SOURCE_GLOB='*.rs'
else
  SOURCE_GLOB='*.zig'
fi

# The registry files are their own allowlist, joined with the test-shaped paths
# every runtime spells differently, plus whatever the caller adds.
REGISTRY_RE="$(printf '%s\n' "${REGISTRY_PATHS[@]}" | sed 's|[.[\*^$/]|\\&|g' | paste -sd'|' -)"
ALLOWLIST_RE="^(${REGISTRY_RE})$|_tests?\\.(zig|rs)$|(^|/)tests?/|(^|/)tests?\\.rs$|/_?test_harness\\.[a-z]+$|_test_fixtures\\.[a-z]+$"
[[ -n "${ORLY_ERROR_ALLOWLIST:-}" ]] && ALLOWLIST_RE="${ALLOWLIST_RE}|${ORLY_ERROR_ALLOWLIST}"

FAIL=0
fail() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()   { printf "OK:   %s\n" "$*"; }
note() { printf "NOTE: %s\n" "$*"; }

# ---------------------------------------------------------------------------
# 1. Extract DECLARED codes from the registry.
#    Pattern: literal "UZ-<CAT>-<NNN>" inside the registry file. <CAT> may
#    contain digits but must start with a letter (e.g. UUIDV7), so the class is
#    [A-Z][A-Z0-9]* — a bare [A-Z]+ stops at the first digit and silently drops
#    the entire UZ-UUIDV7-* family (its category ends in a digit), leaving those
#    codes invisible to the orphan/dead/used passes below.
# ---------------------------------------------------------------------------
declared_codes=$(grep -hoE "${CODE_RE}\b" "${REGISTRY_PATHS[@]}" | sort -u)
if [[ -z "$declared_codes" ]]; then
  fail "no codes declared in ${REGISTRY_PATHS[*]} (registry empty?)"
  exit 1
fi
declared_count=$(printf '%s\n' "$declared_codes" | wc -l | tr -d ' ')

# ---------------------------------------------------------------------------
# 2. Determine USED codes.
#    Source set: src/**/*.zig minus *_test.zig minus the registry itself,
#    plus agentsfleet/src/**.
# ---------------------------------------------------------------------------
gather_used_paths() {
  case "$MODE" in
    --staged|staged)
      git diff --cached --name-only --diff-filter=ACMRT -- $SOURCE_GLOB 2>/dev/null \
        | grep -vE "^(${REGISTRY_RE})$" || true
      ;;
    --all|all)
      git ls-files -- $SOURCE_GLOB 2>/dev/null \
        | grep -vE "^(${REGISTRY_RE})$" || true
      ;;
    *)
      printf "usage: %s [--staged|--all]\n" "$0" >&2
      exit 64
      ;;
  esac
}

# `while read` rather than mapfile: mapfile is bash 4+ and macOS ships 3.2 —
# the portability rule scripts/run-playbook-tests.sh already records.
USED_PATHS=()
while IFS= read -r p; do USED_PATHS+=("$p"); done < <(gather_used_paths)
if [[ ${#USED_PATHS[@]} -eq 0 ]]; then
  ok "no source files in scope ($MODE)"
  exit 0
fi

used_codes=$(awk -v prefix="$PREFIX" '
  # Reset per-file so a trailing intentional-fake marker in the previous
  # file cannot carry skip_next=1 into the first line of the next file.
  FNR == 1 { skip_next = 0; in_rs_test = 0; rs_open = 0; rs_depth = 0 }
  # Rust keeps its unit tests INSIDE the file they cover, under
  # `#[cfg(test)] mod tests { ... }`, so a negative test naming a code it
  # expects to be refused reads as a raw literal in the production half of
  # that same file. Same brace-depth shape as audits/ufs.sh.
  FILENAME ~ /\.rs$/ {
    if (in_rs_test) {
      tmp = $0
      rs_depth += gsub(/\{/, "{", tmp) - gsub(/\}/, "}", tmp)
      if (!rs_open && index($0, "{") > 0) rs_open = 1
      if (rs_open && rs_depth <= 0) in_rs_test = 0
      next
    }
    if ($0 ~ /^[[:space:]]*#\[cfg\(test\)\]/ ||
        $0 ~ /^[[:space:]]*#\[[A-Za-z_:]*test\]/) {
      in_rs_test = 1; rs_open = 0; rs_depth = 0
      next
    }
  }
  # Skip the line carrying the marker AND the next non-blank line (the
  # marker sits on the comment line above the code line in Zig style).
  /audit-error-codes: intentional-fake/ { skip_next = 1; next }
  # A code NAMED in prose is a reference to history, not a call site: Rust doc
  # comments routinely say which code a path used to return. Skipping them here
  # matches the raw-literal scan below, which has always skipped comments.
  /^[[:space:]]*\/\// { next }
  /^[[:space:]]*$/ { next }
  skip_next { skip_next = 0; next }
  {
    line = $0;
    while (match(line, prefix "-[A-Z][A-Z0-9]*-[0-9][0-9][0-9][0-9]*")) {
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
  fail "orphan codes (used but not declared in ${REGISTRY_PATHS[*]}):"
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
raw_leaks=$(awk -v prefix="$PREFIX" '
  FNR == 1 { skip_next = 0; in_rs_test = 0; rs_open = 0; rs_depth = 0 }
  # Rust keeps its unit tests INSIDE the file they cover, under
  # `#[cfg(test)] mod tests { ... }`, so a negative test naming a code it
  # expects to be refused reads as a raw literal in the production half of
  # that same file. Same brace-depth shape as audits/ufs.sh.
  FILENAME ~ /\.rs$/ {
    if (in_rs_test) {
      tmp = $0
      rs_depth += gsub(/\{/, "{", tmp) - gsub(/\}/, "}", tmp)
      if (!rs_open && index($0, "{") > 0) rs_open = 1
      if (rs_open && rs_depth <= 0) in_rs_test = 0
      next
    }
    if ($0 ~ /^[[:space:]]*#\[cfg\(test\)\]/ ||
        $0 ~ /^[[:space:]]*#\[[A-Za-z_:]*test\]/) {
      in_rs_test = 1; rs_open = 0; rs_depth = 0
      next
    }
  }
  # Skip the line carrying the marker AND the next non-blank line.
  /audit-error-codes: intentional-fake/ { skip_next = 1; next }
  /^[[:space:]]*\/\// { next }
  /^[[:space:]]*$/ { next }
  skip_next { skip_next = 0; next }
  # Match any "UZ-XXX(-YYY)*-NNN" string literal.
  $0 ~ "\"" prefix "-[A-Z][A-Z0-9-]*-[0-9]+\"" {
    print FILENAME ":" FNR ":" $0;
  }
' $(printf '%s\n' "${USED_PATHS[@]}" | grep -vE \
    "$ALLOWLIST_RE" || true) 2>/dev/null || true)

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
ok "registry (${RUNTIME:-unknown}): $declared_count declared, $used_count used"
if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 ERROR REGISTRY GATE: violations found. Reference the registry symbol (e.g. \`error_codes.ERR_X\` or \`client_errors.ERR_X\` for the executor crate) instead of duplicating the literal.\n" >&2
  exit 1
fi
ok "ERROR REGISTRY GATE: clean"
exit 0
