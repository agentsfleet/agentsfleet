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

# Default: full-codebase scan (`--all` is an explicit alias). `--staged` is the
# pre-commit lens — it narrows the per-file checks to `git diff --cached`. The
# retired `--diff` (BASE...HEAD) mode stays rejected; `--staged` reads the index
# and so is not blind to staged-but-uncommitted fixes (M70's concern).
MODE="${1:-}"
case "$MODE" in
  ""|--all|all)    MODE="--all" ;;
  --staged|staged) MODE="--staged" ;;
  *)
    printf "usage: %s [--all|--staged]\n" "$0" >&2
    printf "note: --diff was retired in M70 — see dispatch/write_any.md (UFS Gate → Scope). Use --staged for the pre-commit (index) lens.\n" >&2
    exit 2
    ;;
esac
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
if [ "$MODE" = "--staged" ]; then
  scope_files() { git diff --cached --name-only --diff-filter=ACMRT; }
else
  scope_files() { git ls-files; }
fi
# `while read` rather than mapfile: mapfile is bash 4+ and macOS ships 3.2 —
# the portability rule scripts/run-playbook-tests.sh already records.
FILES=()
while IFS= read -r f; do
  FILES+=("$f")
done < <(scope_files | while read -r f; do
  is_source "$f" && echo "$f"
done)

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
done < <(awk '
  FNR == 1 {
    prev_file = FILENAME
    # Language lane. Each lane below carves out what its syntax makes
    # UNFIXABLE — a literal a named const cannot legally replace. Anything
    # a const CAN replace stays in scope, in every language.
    lang = "other"
    if (FILENAME ~ /\.zig$/)             lang = "zig"
    else if (FILENAME ~ /\.rs$/)         lang = "rust"
    else if (FILENAME ~ /\.go$/)         lang = "go"
    # Test files: repetition is fixture data, not magic strings.
    # Skip string-dup-file for tests; other checks still apply.
    is_test = (FILENAME ~ /(_test\.zig|_test\.go|\.test\.|\.spec\.|\.unit\.test|\.integration\.test|\/test\/|\/tests\/)/)
    # Directory-shaped test and benchmark trees. Rust puts both at the CRATE
    # ROOT (`tests/`, `benches/`) with no leading slash, so the `\/tests\/`
    # form above — which needs a parent directory — walked straight past them
    # and audited 100 fixture hits as production literals.
    if (FILENAME ~ "(^|/)(test|tests|benches|__tests__)/") is_test = 1
  }
  is_test { next }
  # ui/ files: extracting class-strings or short literals to file-local
  # consts is fragile in TypeScript (type-position uses widen string|
  # literal types, and the cleanup belongs in a UI-aware spec). Skip
  # string-dup-file for ui/packages/*/src; cross-runtime-orphan still
  # runs against ui/ to catch ERR_* parity drift.
  # macOS/BSD awk aborts on a `/` inside a `[...]` class in a regex *literal*
  # ("nonterminated character class"); use a dynamic-regex STRING — identical
  # match on gawk, portable on BWK awk (the macOS default). Do not re-inline.
  FILENAME ~ "^ui/packages/[^/]+/(src|app|tests|components|lib|hooks)/" { next }
  FNR == 1 { in_test_block = 0; test_depth = 0; in_block_comment = 0; in_rs_test = 0; rs_open = 0; rs_depth = 0; in_go_const = 0 }
  {
    line = $0
    # Block-comment exclusion (TS/JS/Zig non-applicable): skip lines inside /* ... */
    if (in_block_comment) {
      if (line ~ /\*\//) in_block_comment = 0
      next
    }
    if (line ~ /\/\*/ && line !~ /\*\//) {
      in_block_comment = 1
      next
    }
    # Zig multi-line string literal — lines start with `\\` after whitespace.
    if (lang == "zig" && line ~ /^[[:space:]]*\\\\/) next
    # Inline-test exclusion (Zig): track depth across `test "..." {` blocks.
    if (lang == "zig") {
      if (in_test_block) {
        test_depth += gsub(/\{/, "{", line) - gsub(/\}/, "}", line)
        if (test_depth <= 0) in_test_block = 0
        line = $0  # restore
        next
      }
      if (line ~ /^test[[:space:]]+"/) {
        in_test_block = 1
        tmp = line
        test_depth = gsub(/\{/, "{", tmp) - gsub(/\}/, "}", tmp)
        if (test_depth <= 0) in_test_block = 0
        next
      }
    }
    # Inline-test exclusion (Rust): Rust keeps its unit tests INSIDE the file
    # they cover, under `#[cfg(test)] mod tests { ... }`, so without this the
    # fixture keys of a well-tested module read as the worst literal debt in
    # the production half of that same file — a real crate reported `"key1"`
    # 15 times from one such block. Same brace-depth shape as the Zig branch.
    if (lang == "rust") {
      if (in_rs_test) {
        tmp = line
        rs_depth += gsub(/\{/, "{", tmp) - gsub(/\}/, "}", tmp)
        if (!rs_open && index(line, "{") > 0) rs_open = 1
        if (rs_open && rs_depth <= 0) in_rs_test = 0
        else if (!rs_open && line ~ /;[[:space:]]*$/) in_rs_test = 0
        next
      }
      # Three ways Rust marks a block as fixture data, not shipped behaviour:
      # `#[cfg(test)]`, a `#[test]`/`#[tokio::test]` function, and a
      # FEATURE-GATED test seam — `#[cfg(feature = "test-util")]`, the
      # convention for a helper that must compile into the crate so an
      # integration test in a sibling file can call it (a `one_of_each_kind`
      # error-surface enumerator is the usual shape). The feature name is
      # matched narrowly, `test` as a whole word or a `-`/`_` segment, so a
      # production gate like `#[cfg(feature = "redis")]` still counts and
      # `"latest"` is not mistaken for a test seam.
      if (line ~ /^[[:space:]]*#\[cfg\(test\)\]/ ||
          line ~ /^[[:space:]]*#\[[A-Za-z_:]*test\]/ ||
          line ~ /^[[:space:]]*#\[cfg\(.*feature[[:space:]]*=[[:space:]]*"([A-Za-z0-9]+[-_])?test(ing)?([-_][A-Za-z0-9_-]+)?"/) {
        in_rs_test = 1; rs_open = 0; rs_depth = 0
        next
      }
      # Attribute literals are UNFIXABLE, not undisciplined. `#[serde(rename =
      # "...")]`, `#[cfg(feature = "...")]` and `#[doc = "..."]` take a literal
      # token by language rule — a const is not accepted there, so naming one
      # cannot resolve the hit and reporting it only teaches people to ignore
      # the gate. Ordered after the cfg(test) branch, which is also a `#[`.
      if (line ~ /^[[:space:]]*#!?\[/) next
    }
    # Grouped-const exclusion (Go): `const ( ... )` states the binding keyword
    # ONCE, on the opening line, so a member like `metaKeyQoS = "conf_..."` is
    # a definition carrying no `const` for the binding carve-out below to see.
    # `var (` groups are deliberately NOT tracked — the carve-out is const-only
    # in every other language and widening it here would be a silent divergence.
    if (lang == "go") {
      if (in_go_const) {
        if (line ~ /^[[:space:]]*\)/) { in_go_const = 0; next }
        if (line ~ /^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*([[:space:]]+[A-Za-z0-9_.\[\]*]+)?[[:space:]]*=/) next
      }
      else if (line ~ /^[[:space:]]*const[[:space:]]*\([[:space:]]*$/) { in_go_const = 1; next }
    }
    sub(/\/\/.*$/, "", line)
    # Strip single-line /* ... */ inline block comments (jsdoc/etc) so
    # literals inside them are not counted.
    gsub(/\/\*[^*]*\*+([^\/*][^*]*\*+)*\//, "", line)
    # Const-BINDING carve-out (narrower than the numeric-suspect is_const_decl
    # exemption below — deliberately): a literal that IS the right-hand side of a
    # `const`/`pub const`/`export const` binding is its single-source DEFINITION,
    # not a magic-string use. Two distinct named constants may legitimately share
    # a value across domains (e.g. a runner status and a lease status both
    # "active") — RULE UFS targets UN-named repetition, and both sites there are
    # already named. But a literal passed as a CALL ARGUMENT on a const line
    # (`const x = foo("lit");`) is NOT named by that const — it counts and flags
    # like any other use. (A prior line-level skip exempted those too and silently
    # gutted the check for most Zig code; the ufs_dup_string eval fixture pins the
    # binding-level semantic.)
    # `static` joins the keyword set for Rust: `static NAME: &str = "..."` binds
    # a literal to a name exactly as `const` does, and the leading
    # `(^|[[:space:]])` anchor already carries the `pub`/`pub(crate)` prefixes.
    # The second type slot belongs to Go, which spells the type with no colon
    # (`const name string = "..."`), which the colon-only slot could not see.
    if (line ~ /(^|[[:space:]])(pub[[:space:]]+const|export[[:space:]]+const|const|static)[[:space:]]+[A-Za-z_$][A-Za-z0-9_$]*([[:space:]]*:[^=]*|[[:space:]]+[A-Za-z_][A-Za-z0-9_.\[\]*]*)?[[:space:]]*=[[:space:]]*"((\\.)|[^\\"])+"[[:space:]]*(as[[:space:]]+const)?[[:space:]]*[;,]?[[:space:]]*$/) next
    rest = line
    # Strip Zig identifier-escape syntax @"name" — body is an identifier,
    # not a string literal, but the regex below would otherwise match it.
    gsub(/@"[^"]*"/, "", rest)
    # Strip Go backtick runs. They are overwhelmingly struct tags
    # (`json:"id" yaml:"id"`), whose quoted halves are tag SYNTAX addressed by
    # reflection — a const cannot appear inside one, so the repeat is unfixable.
    # This also drops single-line Go raw strings, which is the accepted cost:
    # the alternative reports every `json:"id"` in a wire struct as a violation.
    # A backtick run spanning lines is not tracked (best-effort, as documented).
    if (lang == "go") gsub(/`[^`]*`/, "", rest)
    # Strip empty string literals so the regex below cannot fuse
    # two adjacent Zig literals through their inner gap.
    gsub(/""/, "", rest)
    # Match a quoted string literal honouring \" escapes: open quote,
    # then runs of (backslash+any-char) | (non-backslash-non-quote),
    # then close quote. Min 3 chars total ensures literal body ≥1 char
    # (back-compat with the {2,} body-min from the prior regex).
    while (match(rest, /"((\\.)|[^\\"])+"/)) {
      lit = substr(rest, RSTART, RLENGTH)
      rest = substr(rest, RSTART + RLENGTH)
      if (length(lit) < 4) continue
      if (lit ~ /^"(http|https|file|\/|\\\\|\\\\n)/) continue
      key = FILENAME "\034" lit
      count[key]++
      file_of[key] = FILENAME
      lit_of[key] = lit
    }
  }
  END {
    for (k in count) {
      if (count[k] >= 2) {
        printf "string-dup-file %s %s %d\n", file_of[k], lit_of[k], count[k]
      }
    }
  }
' "${FILES[@]}")

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
NUMERIC_RE='(^|[^0-9A-Za-z_])(1[_e]?0{3,12}|1_000(_000)*|10_000_000|100_000|1024|1048576|3600|86400)([^0-9A-Za-z_]|$)'

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
# cross-runtime contract surface — server error codes a client consumes. Zig is
# the source of truth: every client-side ERR_* must have a matching Zig
# `pub const ERR_*`. Zig-only codes are fine; a server-internal code needs no
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

zig_err=$(parity_codes '^pub const ERR_[A-Z][A-Z0-9_]+[[:space:]]*=' '*.zig')
client_err=$(parity_codes '^export const ERR_[A-Z][A-Z0-9_]+[[:space:]]*=' '*.js' '*.jsx' '*.ts' '*.tsx')

if [ -z "$zig_err" ]; then
  ok "audit-ufs: cross-runtime parity skipped — no Zig ERR_* declared, so this repository has no source of truth to compare against"
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
for v in ${violations[@]+"${violations[@]}"}; do
  printf "  %s\n" "$v" >&2
done
printf "\nResolve by either (1) extract to a named const + replace all sites,\n" >&2
printf "(2) add the matching const in the missing sibling runtime same-commit,\n" >&2
printf "or (3) annotate '// pin test: literal is the contract' on/above the line.\n" >&2
exit 1
