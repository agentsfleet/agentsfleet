#!/usr/bin/env bash
# dispatch/lib.sh — shared framework for all language dispatch.
#
# A dispatch (Garry Tan's sense) is a DISPATCHER: given a touched file, it
# resolves which deterministic gates apply and runs them. It never does the
# work itself — the leaf helpers in audits/ do. This library gives every
# dispatch one identical verdict-block format so they cannot drift.
#
# Three entry points, one source of truth (each dispatch sources this):
#   EXECUTE        dispatch/<lang>.sh <file>      per-edit, latent early-warning
#   CONFORM dispatch/<lang>.sh --staged    end-of-turn aggregate (the anchor)
#   COMMIT         pre-commit → dispatch/<lang>.sh --staged   mechanical backstop
#
# Signal semantics (printed in every row — see DISPATCH_ARCHITECTURE.md, Signal semantics):
#   🟢 GREEN    deterministic check passed                → proceed         (exit 0)
#   🔴 RED      deterministic check failed / helper absent → STOP, fix, rerun (exit 1)
#   🤔 DECIDE   judgment-only; no script can decide        → agent reads §, makes
#               the call, states the verdict in chat; blocks the TURN, not the
#               script (exit 0). 🟡 is reserved for "violations addressed" in
#               HARNESS_VERIFY_OUTPUT.md and is never emitted here.
#   🟣 N/A       delegated check — runs only in the product repo, not dotfiles
#
# Source this, then call:
#   dispatch_init "<LANG>" <ext-glob...>      # e.g. dispatch_init "ZIG" '*.zig'
#   dispatch_resolve_files "$@"               # populates DISPATCH_FILES[]
#   dispatch_length_gate <cap>                # inline length check
#   dispatch_run_helper <CODE> <script.sh>    # delegate to audits/<script.sh>
#   dispatch_delegate <CODE> "<command>"      # print a DELEGATED row (in-repo only)
#   dispatch_judgment <CODE> "<question>"     # print a 🤔 DECIDE (judgment) row
#   dispatch_verdict                          # final ✅/❌ line; exits with status
#
# CODE is a rule code (UFS, FLL, TGU…). Every CODE prints with its gloss from
# DISPATCH_GLOSS so output self-explains — no naked codes (audit-enforced).

set -euo pipefail

# DISPATCH_HOME — where the dispatch scripts physically live. Follows a
# Orly snapshot, so it always locates lib.sh + the leaf
# helpers correctly. This is NOT the repo being checked.
DISPATCH_HOME="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DISPATCH_SCRIPTS="$DISPATCH_HOME/audits"
# TARGET_ROOT — the repo being checked (the one we were invoked from). Derived
# from the CWD's git toplevel, NOT BASH_SOURCE: a symlinked dispatch inside a
# product repo must scope --staged discovery + length checks to THAT repo, not
# dotfiles (DISPATCH_ARCHITECTURE.md §10). Falls back to DISPATCH_HOME outside a
# git work tree.
TARGET_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || printf '%s' "$DISPATCH_HOME")"
DISPATCH_LANG=""
DISPATCH_EXTS=()
DISPATCH_FILES=()
DISPATCH_RC=0
source "$DISPATCH_SCRIPTS/scope.sh"

# Rule-code gloss map — canonical short expansions (full text in RULES.md legend).
# Keep in sync with RULES.md; dispatch-coverage.sh fails on a code with no
# gloss entry. snake-of-record: CODE → "Short Gloss".
# A heredoc of `[CODE]="Gloss"` lines, not declare -A: associative arrays are
# bash 4+ and macOS ships 3.2. The line shape is load-bearing —
# evals/dispatch/coverage.sh greps lib.sh for it (checks f and g).
DISPATCH_GLOSS="$(cat <<'GLOSS'
  [NDC]="No Dead Code"
  [NLR]="No Legacy Retained (touch-it-fix-it)"
  [NLG]="No Legacy compat shims (pre-v0.30.0)"
  [UFS]="Unified Form for Symbols (literals → named consts)"
  [TGU]="Tagged-Union over optional-field structs"
  [PRI]="Prompt-injection Resistance from user Input"
  [ORP]="ORPhan sweep (cross-layer on rename/delete)"
  [FLL]="File & Function Length Limits"
  [TST-NAM]="TeST NAMing (milestone-free)"
  [PUB]="Pub Surface & Struct-Shape"
  [DRAIN]="pg.Conn drain-before-deinit"
  [SQLMOD]="SQL statements live in domain sql.zig"
  [DEINIT]="init/deinit lifecycle pairing"
  [ARCH]="Architecture consult before naming"
  [XCOMPILE]="Cross-compile both linux targets"
  [FSD]="File Shape Decision (file-as-struct vs operations-over-value)"
  [DIDEM]="Deinit IDEMpotency (cleanup double-call safe / single-shot asserted)"
  [TSC]="TypeScript/Bun lint conventions (const, import, naming, anti-patterns)"
  [TSJ]="TypeScript/Bun judgment conventions (Bun-native, file ordering, error style)"
  [UIS]="UI Substitution (design-system primitive over raw HTML)"
  [DTK]="Design ToKens (named token utility over arbitrary value)"
  [SCH]="SCHema teardown (pre-0.30 full removal; no ALTER/DROP/marker)"
  [ITF]="Integration Test Fixtures (real schema, not TEMP-table mock)"
  [LOG]="LOGging discipline (scoped event, error_code, severity, redaction)"
  [MSID]="Milestone-ID ban in source (M{N}_{NNN} / §x.y / T{N} / dim)"
  [ERR]="ERror Registry (UZ-XXX-NNN declared + referenced)"
  [ERR-RS]="Rust ERror discipline (one type per crate, Result alias, no stringified cause)"
  [GRP]="GREptile rule audit (diff vs greptile-learnings/RULES.md codes)"
  [LDC]="Legacy-Design Consult (A remove / B patch / C keep)"
GLOSS
)"

# Look up a gloss; empty string if the code is unknown (audit will catch it).
# Pure parameter expansion — no fork per finding. `[` is escaped so the code
# is matched literally, not as a glob class.
dispatch_gloss() {
  local rest; rest="${DISPATCH_GLOSS#*\[$1\]=\"}"
  [[ "$rest" == "$DISPATCH_GLOSS" ]] && return 0
  printf '%s' "${rest%%\"*}"
}

dispatch_init() {
  DISPATCH_LANG="$1"; shift
  DISPATCH_EXTS=("$@")
}

# Paths this repository's engine wrote, one per line, from the install record.
#
# A managed file is orly's, not the repository's: the next `orly update`
# overwrites it, so a finding against one is a chore nobody can complete. Left
# unfiltered, staging a fresh install put orly's own leaf helpers in front of
# orly's own length gate — audits/logging.sh is 444 lines against a 350 cap —
# and the first commit of an adoption failed on the adoption itself.
dispatch_managed_paths() {
  local config="$TARGET_ROOT/.orly/orly.json"
  [ -f "$config" ] || return 0
  bun -e 'const c = JSON.parse(await Bun.file(process.argv[1]).text());
    if (!Array.isArray(c.managed) || c.managed.some(p => typeof p !== "string" || p.includes("\u0000")))
      throw new Error("invalid managed ownership array");
    for (const p of c.managed) process.stdout.write(p + "\u0000");' "$config"
}

# Resolve targets: explicit file args, or --staged self-discovery via git.
# Explicit arguments are never filtered — naming a file is asking about it.
dispatch_resolve_files() {
  DISPATCH_FILES=()
  audit_scope_init --all "$@"
  audit_index_snapshot "$@"
  DISPATCH_STAGED=0
  if [ "${1:-}" = "--staged" ]; then
    DISPATCH_STAGED=1
    local pathspecs=() e f owned skip managed_file
    for e in "${DISPATCH_EXTS[@]}"; do pathspecs+=("$e"); done
    managed_file="$(mktemp "${TMPDIR:-/tmp}/orly-managed.XXXXXX")"
    if ! dispatch_managed_paths > "$managed_file"; then
      rm -f -- "$managed_file"; exit 2
    fi
    while IFS= read -r -d '' f; do
      case "$f" in */vendor/*|vendor/*|*/third_party/*|third_party/*|*/node_modules/*|node_modules/*|*/dist/*|*/build/*|*/.next/*|*/.zig-cache/*) continue ;; esac
      skip=0
      while IFS= read -r -d '' owned; do [ "$f" != "$owned" ] || skip=1; done < "$managed_file"
      [ "$skip" -eq 0 ] || continue
      DISPATCH_FILES+=("$f")
    done < <(audit_scope_paths "${pathspecs[@]}")
    rm -f -- "$managed_file"
  elif [ "$#" -ge 1 ]; then
    local f matched
    for f in "$@"; do
      matched=0
      for e in "${DISPATCH_EXTS[@]}"; do
        case "$f" in ${e}) matched=1 ;; esac
      done
      if [ "$matched" -eq 1 ]; then DISPATCH_FILES+=("$f")
      else printf 'dispatch: %s does not match %s scope: %s\n' "$DISPATCH_LANG" "${DISPATCH_EXTS[*]}" "$f" >&2; exit 2
      fi
    done
  else
    printf 'usage: dispatch/<lang>.sh <file> [...] | --staged\n' >&2
    exit 2
  fi
}

# write_any.md §Triggers: "If the file extension is ambiguous, the gate FIRES by
# default." Extension globs cannot express that, so an extensionless staged file
# carrying a shebang is added here instead. Universal dispatch only — a language
# dispatch must never claim a file its gates cannot parse.
dispatch_add_shebang_files() {
  [ "${1:-}" = "--staged" ] || return 0
  local f base path firstline owned skip managed_file
  managed_file="$(mktemp "${TMPDIR:-/tmp}/orly-managed.XXXXXX")"
  if ! dispatch_managed_paths > "$managed_file"; then rm -f -- "$managed_file"; exit 2; fi
  while IFS= read -r -d '' f; do
    skip=0
    while IFS= read -r -d '' owned; do [ "$f" != "$owned" ] || skip=1; done < "$managed_file"
    [ "$skip" -eq 0 ] || continue
    base="${f##*/}"
    case "$base" in *.*) continue ;; esac
    path="$TARGET_ROOT/$f"; [ -f "$path" ] || path="$f"; [ -f "$path" ] || continue
    IFS= read -r firstline < "$path" || true
    case "$firstline" in "#!"*) DISPATCH_FILES+=("$f") ;; esac
  done < <(audit_scope_paths)
  rm -f -- "$managed_file"
}

dispatch_header() {
  if [ "${#DISPATCH_FILES[@]}" -eq 0 ]; then
    printf '%s DISPATCH: no files in scope — nothing to dispatch.\n' "$DISPATCH_LANG"
    exit 0
  fi
  printf '%s DISPATCH — %s file(s) in scope\n' "$DISPATCH_LANG" "${#DISPATCH_FILES[@]}"
}

# Inline length gate — intrinsic to the file, deterministic, no git history.
# Code FLL; gloss printed so the human reading a commit knows the rule.
dispatch_length_gate() {
  local cap="$1" f n g; g="$(dispatch_gloss FLL)"
  for f in "${DISPATCH_FILES[@]}"; do
    local path="$TARGET_ROOT/$f"; [ -f "$path" ] || path="$f"; [ -f "$path" ] || continue
    n="$(wc -l < "$path")"; n="${n//[[:space:]]/}"
    if [ "$n" -gt "$cap" ]; then
      printf '  FLL      🔴 %s — %s: %s lines (cap %s) — split before commit\n' "$g" "$f" "$n" "$cap"
      DISPATCH_RC=1
    else
      printf '  FLL      🟢 %s — %s: %s/%s\n' "$g" "$f" "$n" "$cap"
    fi
  done
}

# Delegate to a deterministic leaf helper in audits/. Normalizes the verdict.
# CODE prints with its gloss (no naked codes — audit-enforced).
#
# Leaf helpers receive explicit targets when the caller names files. In
# staged mode the third argument preserves the helper's declared wider scope.
#   dispatch_run_helper UFS    ufs.sh          --all
#   dispatch_run_helper DEINIT deinit-pairs.sh --staged
dispatch_run_helper() {
  local code="$1" script="$2" mode="${3:-}" g; g="$(dispatch_gloss "$code")"
  # An absent DETERMINISTIC helper is 🔴, never a silent 🟣/0: a deleted or
  # un-synced leaf must NOT pass as a green no-op (DISPATCH_ARCHITECTURE.md §10).
  # 🟣 is reserved for dispatch_delegate (checks that legitimately don't run here).
  if [ ! -f "$DISPATCH_SCRIPTS/$script" ]; then
    printf '  %-8s 🔴 %s — DETERMINISTIC helper absent (audits/%s) — cannot verify\n' "$code" "$g" "$script"
    DISPATCH_RC=1
    return 1
  fi
  local receipt_dir log args=()
  receipt_dir="$(mktemp -d "${TMPDIR:-/tmp}/orly-dispatch.XXXXXX")" || return 1
  log="$receipt_dir/receipt.log"
  if [ "$DISPATCH_STAGED" -eq 1 ]; then [ -z "$mode" ] || args+=("$mode")
  else args=("${DISPATCH_FILES[@]}"); fi
  if (umask 077; bash "$DISPATCH_SCRIPTS/$script" ${args[@]+"${args[@]}"} >"$log" 2>&1); then
    printf '  %-8s 🟢 %s — pass (audits/%s); receipt: %s\n' "$code" "$g" "$script" "$log"
    cat "$log"
  else
    printf '  %-8s 🔴 %s — fail (audits/%s %s) — see %s\n' "$code" "$g" "$script" "$mode" "$log"
    DISPATCH_RC=1
  fi
}

# A gate that can only run inside the project repo (not dotfiles). Print, don't run.
dispatch_delegate() {
  local code="$1" cmd="$2" g; g="$(dispatch_gloss "$code")"
  printf '  %-8s 🟣 %s — DELEGATED → %s\n' "$code" "$g" "$cmd"
}

# A judgment gate that NO script can pass/fail (architecture, legacy-design,
# greptile, tagged-unions). 🤔 DECIDE = open question, NOT a failure: exit stays 0
# but the TURN is incomplete until the agent states a verdict in chat (Signal semantics).
# 🤔 (not 🟡) so it never collides with HARNESS_VERIFY's "violations addressed" 🟡.
# Faking determinism on a taste decision is the anti-goal.
dispatch_judgment() {
  local code="$1" question="$2" g; g="$(dispatch_gloss "$code")"
  printf '  %-8s 🤔 DECIDE — %s: %s\n' "$code" "$g" "$question"
}

dispatch_verdict() {
  if [ "$DISPATCH_RC" -eq 0 ]; then
    printf '%s DISPATCH: ✅ all dispatched gates pass (delegations pending in-repo)\n' "$DISPATCH_LANG"
  else
    printf '%s DISPATCH: ❌ one or more gates failed — fix before commit\n' "$DISPATCH_LANG"
  fi
  exit "$DISPATCH_RC"
}
