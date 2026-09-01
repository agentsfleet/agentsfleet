#!/usr/bin/env bash
# The string-duplication half of the UFS gate.
#
# Sourced by `audits/ufs.sh`, never run on its own: it reads that file's `FILES`
# array and reports through its `record`. Split out at the length cap along the
# gate's own seam — this half asks whether a LITERAL repeats, the half left
# behind asks whether a NUMBER is a disguised constant, and the two share only
# the file list. The awk program below is the reason the file was oversize.

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
