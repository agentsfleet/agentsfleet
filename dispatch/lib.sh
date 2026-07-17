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
#   🔵 DECIDE   judgment-only; no script can decide        → agent reads §, makes
#               the call, states the verdict in chat; blocks the TURN, not the
#               script (exit 0). 🟡 is reserved for "violations addressed" in
#               HARNESS_VERIFY_OUTPUT.md and is never emitted here.
#   ⚪ N/A       delegated check — runs only in the product repo, not dotfiles
#
# Source this, then call:
#   dispatch_init "<LANG>" <ext-glob...>      # e.g. dispatch_init "ZIG" '*.zig'
#   dispatch_resolve_files "$@"               # populates DISPATCH_FILES[]
#   dispatch_length_gate <cap>                # inline length check
#   dispatch_run_helper <CODE> <script.sh>    # delegate to audits/<script.sh>
#   dispatch_delegate <CODE> "<command>"      # print a DELEGATED row (in-repo only)
#   dispatch_judgment <CODE> "<question>"     # print a 🔵 DECIDE (judgment) row
#   dispatch_verdict                          # final ✅/❌ line; exits with status
#
# CODE is a rule code (UFS, FLL, TGU…). Every CODE prints with its gloss from
# DISPATCH_GLOSS so output self-explains — no naked codes (audit-enforced).

set -euo pipefail

# DISPATCH_HOME — where the dispatch scripts physically live. Follows a
# oracle-rules snapshot, so it always locates lib.sh + the leaf
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

# Rule-code gloss map — canonical short expansions (full text in RULES.md legend).
# Keep in sync with RULES.md; dispatch-coverage.sh fails on a code with no
# gloss entry. snake-of-record: CODE → "Short Gloss".
declare -A DISPATCH_GLOSS=(
  [NDC]="No Dead Code"
  [NLR]="No Legacy Retained (touch-it-fix-it)"
  [NLG]="No Legacy compat shims (pre-v2.0.0)"
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
  [SCH]="SCHema teardown (pre-2.0 full removal; no ALTER/DROP/marker)"
  [ITF]="Integration Test Fixtures (real schema, not TEMP-table mock)"
  [LOG]="LOGging discipline (scoped event, error_code, severity, redaction)"
  [MSID]="Milestone-ID ban in source (M{N}_{NNN} / §x.y / T{N} / dim)"
  [ERR]="ERror Registry (UZ-XXX-NNN declared + referenced)"
  [GRP]="GREptile rule audit (diff vs greptile-learnings/RULES.md codes)"
  [LDC]="Legacy-Design Consult (A remove / B patch / C keep)"
)

# Look up a gloss; empty string if the code is unknown (audit will catch it).
dispatch_gloss() { printf '%s' "${DISPATCH_GLOSS[$1]:-}"; }

dispatch_init() {
  DISPATCH_LANG="$1"; shift
  DISPATCH_EXTS=("$@")
}

# Resolve targets: explicit file args, or --staged self-discovery via git.
dispatch_resolve_files() {
  DISPATCH_FILES=()
  if [ "${1:-}" = "--staged" ]; then
    local pathspecs=() e
    for e in "${DISPATCH_EXTS[@]}"; do pathspecs+=("$e"); done
    while IFS= read -r f; do [ -n "$f" ] && DISPATCH_FILES+=("$f"); done \
      < <(git -C "$TARGET_ROOT" diff --cached --name-only --diff-filter=ACMRT -- "${pathspecs[@]}" \
          | grep -vE '(^|/)(vendor|third_party|node_modules|\.zig-cache|dist|build|\.next)/' || true)
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
# Leaf helpers have NON-UNIFORM arg contracts (e.g. audit-deinit-pairs takes
# --staged; audit-ufs takes --all/no-arg after M70). The caller MUST pass the
# mode the helper actually accepts as the 3rd arg — never assume --staged.
#   dispatch_run_helper UFS    ufs.sh          --all
#   dispatch_run_helper DEINIT deinit-pairs.sh --staged
dispatch_run_helper() {
  local code="$1" script="$2" mode="${3:-}" g; g="$(dispatch_gloss "$code")"
  # An absent DETERMINISTIC helper is 🔴, never a silent ⚪/0: a deleted or
  # un-synced leaf must NOT pass as a green no-op (DISPATCH_ARCHITECTURE.md §10).
  # ⚪ is reserved for dispatch_delegate (checks that legitimately don't run here).
  if [ ! -f "$DISPATCH_SCRIPTS/$script" ]; then
    printf '  %-8s 🔴 %s — DETERMINISTIC helper absent (audits/%s) — cannot verify\n' "$code" "$g" "$script"
    DISPATCH_RC=1
    return 1
  fi
  local log="/tmp/dispatch-${DISPATCH_LANG}-${code}.log"
  if bash "$DISPATCH_SCRIPTS/$script" $mode >"$log" 2>&1; then
    printf '  %-8s 🟢 %s — pass (audits/%s %s)\n' "$code" "$g" "$script" "$mode"
  else
    printf '  %-8s 🔴 %s — fail (audits/%s %s) — see %s\n' "$code" "$g" "$script" "$mode" "$log"
    DISPATCH_RC=1
  fi
}

# A gate that can only run inside the project repo (not dotfiles). Print, don't run.
dispatch_delegate() {
  local code="$1" cmd="$2" g; g="$(dispatch_gloss "$code")"
  printf '  %-8s ⚪ %s — DELEGATED → %s\n' "$code" "$g" "$cmd"
}

# A judgment gate that NO script can pass/fail (architecture, legacy-design,
# greptile, tagged-unions). 🔵 DECIDE = open question, NOT a failure: exit stays 0
# but the TURN is incomplete until the agent states a verdict in chat (Signal semantics).
# 🔵 (not 🟡) so it never collides with HARNESS_VERIFY's "violations addressed" 🟡.
# Faking determinism on a taste decision is the anti-goal.
dispatch_judgment() {
  local code="$1" question="$2" g; g="$(dispatch_gloss "$code")"
  printf '  %-8s 🔵 DECIDE — %s: %s\n' "$code" "$g" "$question"
}

dispatch_verdict() {
  if [ "$DISPATCH_RC" -eq 0 ]; then
    printf '%s DISPATCH: ✅ all dispatched gates pass (delegations pending in-repo)\n' "$DISPATCH_LANG"
  else
    printf '%s DISPATCH: ❌ one or more gates failed — fix before commit\n' "$DISPATCH_LANG"
  fi
  exit "$DISPATCH_RC"
}
