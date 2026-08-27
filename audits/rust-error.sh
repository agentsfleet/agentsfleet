#!/usr/bin/env bash
# rust-error.sh — the mechanical half of RULE ERR-RS (Rust error discipline).
#
# Dispatch façade: dispatch/write_rust.md (Error discipline). Pairs with
# dispatch/write_rust.sh, which is what runs it.
#
# The rule has four clauses. Two of them a machine can decide from the text
# alone, and only those two live here:
#
#   1. LOSSY MAP_ERR — `map_err` whose closure converts its own cause through
#      `to_string()` or `format!`. The rule allows `map_err` to ADD context the
#      call site alone knows; it forbids stringifying the cause on the way into
#      an error type, because `to_string()` destroys the `source()` chain. The
#      closure's BINDING is what makes this decidable: `.map_err(|e| Mine::Db(
#      e.to_string()))` stringifies the cause, while `.map_err(|e| Mine::Read {
#      source: e, path: path.to_string() })` stringifies a path and is correct.
#      Only a `to_string()` or `format!` applied to the closure's own parameter
#      is a finding.
#
#   2. MISSING RESULT ALIAS — a crate declaring `pub enum Error` or
#      `pub struct Error` with no `pub type Result<` anywhere beside it. The
#      rule is "one error type per crate, and a `Result` alias beside it": a
#      reader must never have to check WHICH error a signature returns.
#
# The other two clauses stay JUDGMENT and are not attempted here. Whether a
# `source()` implementation returns its own kind needs the Display body read
# beside it, and whether a context addition is warranted is a call-site
# question. A gate that guesses at those gets switched off within a week, and a
# switched-off gate protects nothing — partial mechanisation stated honestly is
# the whole design (the same call audits/doc-read.sh makes about its record).
#
# Carve-outs match audits/logging.sh exactly, so one Rust scope answers to both
# leaves: build scripts, `tests/`, `benches/`, `examples/`, and `#[cfg(test)]`
# modules are fixture code, where a stringified error in a test assertion is
# not a production error path.
#
# Modes:
#   --staged   diff-scope: only *.rs in `git diff --cached`
#   --all      (default) every tracked/unignored *.rs
#
# Exits 0 clean, 1 on findings.

set -euo pipefail

MODE_STAGED="--staged"
MODE_ALL="--all"
RULE="ERR-RS"
FACADE="dispatch/write_rust.md"
# The closure body window. A `map_err` closure that has not closed its
# parentheses within this many lines is a block long enough that its conversion
# is no longer a one-liner, and the rule's subject is the one-liner.
MAX_CLOSURE_LINES=12
CARGO_MANIFEST="Cargo.toml"
RESULT_ALIAS_PATTERN='^[[:space:]]*pub[[:space:]]+type[[:space:]]+Result[[:space:]]*<'
ERROR_TYPE_PATTERN='^[[:space:]]*pub[[:space:]]+(enum|struct)[[:space:]]+Error[[:space:]]*[<{(;]'

MODE="$MODE_ALL"
for arg in "$@"; do
  case "$arg" in
    --staged|staged) MODE="$MODE_STAGED" ;;
    --all|all)       MODE="$MODE_ALL" ;;
    -h|--help)
      printf 'usage: %s [--staged|--all]\n' "$0"
      exit 0
      ;;
    *) printf 'unknown arg: %s\n' "$arg" >&2; exit 64 ;;
  esac
done

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

FAIL=0
fail() { printf 'FAIL: %s\n' "$*" >&2; FAIL=1; }
ok()   { printf 'OK:   %s\n' "$*"; }

# Non-runtime Rust paths, identical to audits/logging.sh's carve-out list.
is_non_runtime_rust_path() {
  case "$1" in
    build.rs|*/build.rs|tests/*|*/tests/*|benches/*|*/benches/*|examples/*|*/examples/*) return 0 ;;
  esac
  return 1
}

gather_paths() {
  case "$MODE" in
    "$MODE_STAGED")
      git diff --cached --name-only --diff-filter=ACMRT -- '*.rs' || true
      ;;
    "$MODE_ALL")
      git ls-files --cached --others --exclude-standard -- '*.rs' || true
      ;;
  esac
}

FILES=()
while IFS= read -r path; do
  [[ -n "$path" && -f "$path" ]] || continue
  is_non_runtime_rust_path "$path" && continue
  FILES+=("$path")
done < <(gather_paths)

if [[ ${#FILES[@]} -eq 0 ]]; then
  ok "$RULE: no runtime Rust files in scope ($MODE)"
  exit 0
fi

# ---------------------------------------------------------------------------
# 1. Scan every in-scope file once, emitting both signals.
# ---------------------------------------------------------------------------
# One pass, one carve-out. Both halves need the same `#[cfg(test)]` skip, and
# implementing it twice is how the two halves would come to disagree about what
# a test module is — so the scanner emits `LOSSY:` and `ERRTYPE:` records from a
# single walk and the shell decides what to do with each.
#
# The lossy scanner accumulates a map_err closure from its opening parenthesis
# until the parentheses balance, so a conversion split over several lines reads
# the same as a one-liner.
signals="$(awk -v maxlines="$MAX_CLOSURE_LINES" -v errtype="$ERROR_TYPE_PATTERN" '
  FNR == 1 { intest = 0; testdepth = 0; pending = 0; collecting = 0 }

  # A #[cfg(test)] module is fixture code. The attribute precedes the item, so
  # arm on the attribute and start counting braces at the item that follows.
  !collecting && /#\[cfg\(test\)\]/ { pending = 1; next }
  pending {
    if (index($0, "{") > 0) { pending = 0; intest = 1; testdepth = 0 }
    else next
  }
  intest {
    n = gsub(/\{/, "{"); m = gsub(/\}/, "}")
    testdepth += n - m
    if (testdepth <= 0) intest = 0
    next
  }

  {
    line = $0
    # Strip a trailing line comment so a commented example is not a finding.
    sub(/\/\/.*$/, "", line)

    # A public error type outside any test module. Emitted per file; the shell
    # resolves it to a crate and asks that crate for its alias once.
    if (!collecting && line ~ errtype) printf "ERRTYPE:%s\n", FILENAME

    if (!collecting) {
      where = match(line, /map_err[[:space:]]*\([[:space:]]*\|[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\|/)
      if (where == 0) next
      rest = substr(line, where)
      # The closure binding: the identifier between the pipes.
      bind = rest
      sub(/^map_err[[:space:]]*\([[:space:]]*\|[[:space:]]*/, "", bind)
      sub(/[[:space:]]*\|.*$/, "", bind)
      if (bind == "_") next
      collecting = 1; buffer = rest; startline = FNR; spanned = 1
      depth = gsub(/\(/, "(", rest) - gsub(/\)/, ")", rest)
    } else {
      buffer = buffer " " line
      spanned++
      depth += gsub(/\(/, "(", line) - gsub(/\)/, ")", line)
    }

    if (depth > 0 && spanned < maxlines) next

    # `bind.to_string()` — the cause converted to text on the way in.
    stringified = 0
    if (buffer ~ ("(^|[^A-Za-z0-9_])" bind "[[:space:]]*\\.[[:space:]]*to_string[[:space:]]*\\(")) stringified = 1
    # `format!(... bind ...)` — the same loss wearing a formatter.
    if (buffer ~ /format!/ && buffer ~ ("(^|[^A-Za-z0-9_])" bind "([^A-Za-z0-9_]|$)")) {
      after = buffer; sub(/^.*format!/, "", after)
      if (after ~ ("(^|[^A-Za-z0-9_])" bind "([^A-Za-z0-9_]|$)")) stringified = 1
    }
    if (stringified) printf "LOSSY:%s:%d\n", FILENAME, startline
    collecting = 0
  }
' "${FILES[@]}" || true)"

lossy_count=0
error_type_files=()
while IFS= read -r signal; do
  case "$signal" in
    LOSSY:*)
      lossy_count=$((lossy_count + 1))
      fail "${signal#LOSSY:}  $RULE: map_err stringifies its own cause — to_string()/format! on the closure binding destroys the source() chain. Carry the cause with #[from] or a source field; map_err only ADDS context."
      ;;
    ERRTYPE:*)
      error_type_files+=("${signal#ERRTYPE:}")
      ;;
  esac
done <<< "$signals"

# ---------------------------------------------------------------------------
# 2. An error type with no Result alias beside it.
# ---------------------------------------------------------------------------
# Crate-scoped: the alias may live in any file of the crate that declares the
# error, so the question is asked of the crate, once, not of the file.
crate_root_of() {
  local dir; dir="$(dirname "$1")"
  while [[ "$dir" != "." && "$dir" != "/" ]]; do
    [[ -f "$dir/$CARGO_MANIFEST" ]] && { printf '%s' "$dir"; return 0; }
    dir="$(dirname "$dir")"
  done
  [[ -f "$CARGO_MANIFEST" ]] && { printf '.'; return 0; }
  return 1
}

alias_count=0
checked_crates=""
for path in ${error_type_files[@]+"${error_type_files[@]}"}; do
  crate=""
  crate_root_of "$path" > /dev/null 2>&1 && crate="$(crate_root_of "$path")"
  # A .rs file outside any Cargo crate has no crate to carry the alias; the
  # lossy-map_err half still judged it.
  [[ -n "$crate" ]] || continue
  case " $checked_crates " in *" $crate "*) continue ;; esac
  checked_crates="$checked_crates $crate"
  if ! git ls-files --cached --others --exclude-standard -- "$crate/*.rs" \
       | xargs grep -lE "$RESULT_ALIAS_PATTERN" 2>/dev/null | grep -q .; then
    alias_count=$((alias_count + 1))
    fail "$path  $RULE: crate '$crate' declares a public Error type with no \`pub type Result<T, E = Error>\` beside it — a reader must not have to check WHICH error a signature returns."
  fi
done

ok "$RULE: scanned ${#FILES[@]} runtime Rust file(s); lossy-map_err=$lossy_count missing-result-alias=$alias_count"
if [[ $FAIL -ne 0 ]]; then
  printf '\n🔴 %s: blocking violations. See %s (Error discipline).\n' "$RULE" "$FACADE" >&2
  exit 1
fi
exit 0
