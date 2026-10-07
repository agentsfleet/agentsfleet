#!/usr/bin/env bash
# ufs.sh — enforce RULE UFS (Unified Form for Symbols) across the worktree.
#
# Dispatch façade: dispatch/write_any.md (UFS Gate)
# Fires in: make lint, CONFORM.
#
# Languages read: Zig, TypeScript, JavaScript, Rust, Go (see `is_source`, which
# carries the pack that owns each extension and the reasons `.py` / `.sh` are
# held out). The set is pinned against the dispatch facade by the engine
# dispatch-coverage audit — a facade that fires on a language its leaf cannot
# read is a gate that prints green over an unscanned file.
#
# Generic detection — no manifest of known literals, so the audit scales
# as the codebase grows. Three classes of violation:
#
#   1. string-dup-file   — same string literal ≥2× in one source file
#   2. numeric-suspect   — power-of-ten or unit-factor numeric not bound
#                          to a const, not on a pin-test carve-out line
#   3. cross-runtime-orphan — SCREAMING_SNAKE const defined in one runtime
#                          but missing from a sibling runtime the diff touches
#
# Carve-out: any `// pin test: literal is the contract` comment on or
# above the offending line excludes that line from numeric-suspect.
# A `const` / `pub const` / `export const` / `static` declaration line is
# likewise exempt for both per-file checks (is_const_decl + its string-dup
# mirror below) — binding the literal to a name on that line clears the hit.
# Go states the keyword once for a whole `const ( ... )` group; the string-dup
# lane tracks the group so its members are read as the declarations they are.
#
# Staged-scope semantics: --staged narrows WHICH FILES are scanned, not
# how much of each — a staged file is audited in FULL, so staging a file
# for an unrelated one-line change drags its pre-existing literal debt
# into the commit. Broad sweeps (renames touching N files) surface N
# files' latent hits at once; plan that cleanup before staging.
#
# Scope (M70):
#   Walks the full working tree via `git ls-files` — sees staged content
#   because the index is what `ls-files` reports. Pre-commit-safe: a fix
#   staged but not yet committed satisfies the check on the same hook run.
#   Deterministic negative fixtures under `evals/dispatch/fixtures/` are
#   excluded from storage scans. The dispatch evaluator copies each fixture
#   into `src/` before asserting its expected pass or failure result.
#   The previous `--diff` (BASE...HEAD) mode was retired with M70 because
#   it was blind to the index at pre-commit time.
#
# Usage:
#   ufs.sh           # full-codebase scan (default)
#   ufs.sh --all     # alias for default
#   ufs.sh --staged  # narrow per-file checks (string-dup, numeric) to
#                    # `git diff --cached`; cross-runtime parity stays full-tree
#
# Exits 0 clean, 1 on any blocking violation.

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/scope.sh"
audit_scope_init --all "$@"
audit_index_snapshot '*.zig' '*.ts' '*.tsx' '*.js' '*.jsx' '*.rs' '*.go'

# Default: full-codebase scan (`--all` is an explicit alias). `--staged` is the
# pre-commit lens — it narrows the per-file checks to `git diff --cached`. The
# retired `--diff` (BASE...HEAD) mode stays rejected; `--staged` reads the index
# and so is not blind to staged-but-uncommitted fixes (M70's concern).
MODE="$AUDIT_MODE"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

FAIL=0
violations=()
record() { violations+=("$*"); FAIL=1; }
ok()     { printf "OK:   %s\n" "$*"; }

# ── File scope ──────────────────────────────────────────────────────────────

# The extension set is the CONTRACT the dispatch façade already advertises:
# dispatch/write_any.sh's `dispatch_init "ANY" ...` list is what fires the gate,
# and this list is what the gate then reads. When the two disagree the façade
# runs, prints green, and has scanned nothing — which is exactly how a Rust
# crate carried `const S_PING = "PING"` past a gate that claimed to cover it.
# The engine fails its own dispatch-coverage audit on that drift now, so a
# language added to the façade must land here, or be named out of scope with a
# reason, before that audit goes green again.
#
#   .zig                     language.zig
#   .ts .tsx                 language.typescript
#   .js .jsx                 language.javascript
#   .rs                      language.rust
#   .go                      language.go
#
# Out of scope, deliberately — the engine keeps the machine-readable twin of
# this list: `.py` (single-quoted literals dominate and the
# double-quote matcher below would report partial coverage as full), `.sh`
# (a repeated `"$var"` is interpolation, not a magic string — measured at 59%
# of hits on a real tree), `.sql` (its own gate, dispatch/write_sql.sh).
is_source() {
  local f="$1"
  case "$f" in
    vendor/*|third_party/*|.zig-cache/*|*/node_modules/*|evals/dispatch/fixtures/*|*.tsbuildinfo) return 1 ;;
    *_test.zig|*.test.ts|*.test.tsx|*.test.js|*.test.jsx|*.unit.test.js|*.spec.ts|*_test.go) ;; # tests in scope
  esac
  case "$f" in
    *.zig|*.ts|*.tsx|*.js|*.jsx|*.rs|*.go) return 0 ;;
    *) return 1 ;;
  esac
}

# Per-file check scope (string-dup-file, numeric-suspect). --staged narrows to
# the commit; cross-runtime-orphan (below) always scans the full tree.
FILES=()
while IFS= read -r -d '' f; do
  if is_source "$f" && [ -f "$f" ]; then FILES+=("$f"); fi
done < <(audit_scope_paths '*.zig' '*.ts' '*.tsx' '*.js' '*.jsx' '*.rs' '*.go')

[ "${#FILES[@]}" -eq 0 ] && { ok "audit-ufs: no source files in scope"; exit 0; }

# ── 1. string-dup-file ──────────────────────────────────────────────────────
# Extract double-quoted string literals (best-effort regex; ignores escapes
# inside strings — fine for this discipline check, not a parser). Skip
# 1-char strings, common short labels, doc-strings, JSON keys.

# Perf (M70): single awk over all files instead of per-file pipeline
# (was 5 forks × ~760 files = ~20s). Group counts by FILENAME so the
# "≥2 occurrences in one file" semantic is preserved.
#
# Subshell-propagation fix (M70): the loop reads from a process
# substitution (not a pipe) so `record` mutates FAIL / violations in
# the parent shell. The previous pipe-into-while form ran the loop in
# a subshell and silently dropped violations.
while IFS= read -r row; do
  record "$row"
done < <(awk -f "$AUDIT_HOME/ufs-strings.awk" "${FILES[@]}")

# ── 2. numeric-suspect ──────────────────────────────────────────────────────
# Flag bare power-of-ten and unit-factor numerics in expressions.
# Pattern matches: 1_000, 1_000_000, 1_000_000_000, 1e3, 1e6, 1e9,
# 60, 3600, 86400, 1024, 1048576, 10_000_000, 100_000.
# A line is a violation if:
#  - it contains one of those patterns
#  - the line is NOT a const declaration (`pub const`, `export const`, `const`)
#  - the line is NOT marked `// pin test: literal is the contract`
#  - the line above is NOT marked `// pin test: literal is the contract`

# Boundaries are capturing groups, not \b: macOS/BSD awk treats \b as a backspace,
# so word-boundary anchors silently match nothing. The leading/trailing boundary
# chars are trimmed back off the extracted token below (see RSTART/RLENGTH use).
NUMERIC_RE='(^|[^0-9A-Za-z_])(1[_e]?0{3,12}|1_000(_000)*|10_000_000|100_000|1024|1048576|60|3600|86400)([^0-9A-Za-z_]|$)'

# Perf (M70): single awk across all files; FNR == 1 detects file boundary
# so the "prev line carve-out" stays per-file. Process substitution keeps
# the while loop in the parent shell so record() mutates FAIL.
while IFS= read -r row; do
  record "$row"
done < <(awk -v re="$NUMERIC_RE" '
  FNR == 1 { prev = "" }
  {
    line = $0
    is_pin_now   = (line ~ /pin test: literal is the contract/)
    is_pin_above = (prev ~ /pin test: literal is the contract/)
    is_const_decl = (line ~ /(^|[[:space:]])(pub[[:space:]]+const|export[[:space:]]+const|const|static)[[:space:]]/)
    stripped = line
    sub(/\/\/.*$/, "", stripped)
    sub(/#.*$/, "", stripped)
    if (!is_pin_now && !is_pin_above && !is_const_decl && match(stripped, re)) {
      tok = substr(stripped, RSTART, RLENGTH)
      gsub(/^[^0-9]+|[^0-9_e]+$/, "", tok)   # trim the captured boundary chars
      printf "numeric-suspect %s:%d %s\n", FILENAME, FNR, tok
    }
    prev = line
  }
' "${FILES[@]}")

# ── 3. cross-runtime-orphan ─────────────────────────────────────────────────
# Full-tree ERR_* parity. Scoped to the ERR_* prefix because that is the
# cross-runtime contract surface — server error codes a client consumes. The
# daemon is the source of truth. A Rust daemon declares its codes through a
# registry — `ErrorCode::declare("…")` — and the contract a client holds is
# that STRING, so parity is checked on the string: every client
# `export const ERR_X = "…"` must name a code the registry declares. Where no
# registry exists and Zig does, the Zig `pub const ERR_*` NAMES are the
# oracle instead. Daemon-only codes are fine; a server-internal code needs no
# client mirror.
#
# Scoped BY RUNTIME, not by one repository's directory names. The previous form
# globbed `src/*.zig`, `agentsfleet/src/*.{js,jsx,ts,tsx}` and
# `ui/packages/*/src/*.ts{,x}` — one repository's layout inside a gate every
# repository receives, and it aged exactly as badly as that implies.
# `agentsfleet/src/` stopped existing, so the JavaScript half globbed ZERO
# files and "every JS ERR_* has a Zig twin" passed by scanning nothing, while
# three real codes one directory over went uncompared. A check aimed at a path
# that resolves to nothing is the same silent green this audit exists to catch.
#
# Zig is the source of truth WHERE ZIG EXISTS. A repository with no Zig has
# nothing to compare against, so the check reports that it skipped rather than
# passing vacuously — and, more to the point, does not turn every error code in
# a pure-TypeScript repository into an orphan the moment the globs get fixed.
#
# Perf (M70): batched `xargs grep` (single process across all files) instead of
# `xargs -I{} grep` (one process per file). The server-side scan dropped from
# ~30s to <2s on a 1000-file Zig tree; scoping by runtime keeps that shape.

# Test and fixture trees are excluded DIRECTORY-shaped, not by `.test.` infix
# alone. `cli/test/acceptance/fixtures/install-negatives-ops.ts` declares codes
# that are INPUTS to a negative test — an infix-only filter walked straight
# past it and would have reported two fixtures as production orphans.
PARITY_EXCLUDE='(^|/)(test|tests|__tests__|fixtures)/|_test\.zig$|\.test\.|\.spec\.|(^|/)node_modules/|^vendor/|^third_party/|^src/zbench_fixtures\.zig$'

parity_codes() {
  local declaration="$1"; shift
  git ls-files -z -- "$@" 2>/dev/null \
    | { grep -zvE "$PARITY_EXCLUDE" || true; } \
    | { xargs -0 grep -hE "$declaration" 2>/dev/null || true; } \
    | grep -oE 'ERR_[A-Z][A-Z0-9_]+' | sort -u || true
}

# The Rust registry outranks a Zig mirror: a runner that keeps a Zig subset of
# the daemon's codes for its own use is a client, not the oracle. `record`
# mutates globals, so the loop runs in this shell over a here-string — never on
# the right of a pipe, which would run it in a subshell and drop every finding.
rust_codes=$(git ls-files -z -- '*.rs' 2>/dev/null \
  | { grep -zvE "$PARITY_EXCLUDE" || true; } \
  | { xargs -0 grep -hoE 'ErrorCode::declare\("[^"]+"\)' 2>/dev/null || true; } \
  | { grep -oE '"[^"]+"' || true; } | tr -d '"' | sort -u)
zig_err=$(parity_codes '^pub const ERR_[A-Z][A-Z0-9_]+[[:space:]]*=' '*.zig')
client_err=$(parity_codes '^export const ERR_[A-Z][A-Z0-9_]+[[:space:]]*=' '*.js' '*.jsx' '*.ts' '*.tsx')

if [ -n "$rust_codes" ]; then
  client_decls=$(git ls-files -z -- '*.js' '*.jsx' '*.ts' '*.tsx' 2>/dev/null \
    | { grep -zvE "$PARITY_EXCLUDE" || true; } \
    | { xargs -0 grep -hE '^export const ERR_[A-Z][A-Z0-9_]+[[:space:]]*=[[:space:]]*"[^"]+"' 2>/dev/null || true; })
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    name=$(printf '%s' "$line" | grep -oE 'ERR_[A-Z][A-Z0-9_]+' | head -1)
    code=$(printf '%s' "$line" | sed -E 's/^[^=]*=[[:space:]]*"([^"]+)".*/\1/')
    echo "$rust_codes" | grep -qxF -- "$code" || record "cross-runtime-orphan $name ($code) absent-in-daemon"
  done <<<"$client_decls"
elif [ -z "$zig_err" ]; then
  ok "audit-ufs: cross-runtime parity skipped — no Rust registry and no Zig ERR_* declared, so this repository has no source of truth to compare against"
else
  # Every client-side ERR_* must exist in Zig.
  for c in $client_err; do
    if ! echo "$zig_err" | grep -qx "$c"; then
      record "cross-runtime-orphan $c absent-in-zig"
    fi
  done
fi

# ── Report ──────────────────────────────────────────────────────────────────

if [ "$FAIL" -eq 0 ]; then
  ok "audit-ufs: no violations across ${#FILES[@]} file(s)"
  exit 0
fi

printf "🔴 UFS GATE — %d violation(s):\n" "${#violations[@]}" >&2
for v in "${violations[@]}"; do
  printf "  %s\n" "$v" >&2
done
printf "\nResolve by either (1) extract to a named const + replace all sites,\n" >&2
printf "(2) add the matching const in the missing sibling runtime same-commit,\n" >&2
printf "or (3) annotate '// pin test: literal is the contract' on/above the line.\n" >&2
exit 1
