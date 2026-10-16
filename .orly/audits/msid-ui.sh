#!/usr/bin/env bash
# msid-ui.sh — single awk pass over the diff, running the two
# discipline checks that share a regex pass (MS-ID + UI substitution).
# Previously `combined.sh`; renamed once the PUB clause moved to
# zlint + agent chat-output discipline (see commit history).
#
# Emits per-line hits for:
#   * MS-ID — milestone-id leak in source/config (M{N}_{NNN}, §X.Y, T{N},
#             dim {N}.{N}) outside docs/ + node_modules/ + vendor/.
#             Test files are NOT exempt (RULE TST-NAM).
#   * UI    — raw HTML element in ui/packages/app/*.{tsx,jsx} where a
#             design-system primitive exists (UI Component Substitution Gate).
#
# Non-empty hit list → exit 1. Otherwise exit 0 + one OK line.
#
# Note on PUB:
#   This script no longer flags `pub` declarations. The PUB GATE
#   (dispatch/write_zig.md, pub-surface) has two mechanical enforcers and one
#   human one; this script was a fourth, redundant layer:
#     * Orphan check ("is anyone using this pub?") → zlint's
#       `unused-decls: error` rule, run by `make lint` later in the
#       same pre-commit pass. A `pub` with no in-tree consumer fails
#       there. The audit's regex was duplicating this signal with
#       false positives (e.g. variant lines inside private blocks)
#       and no way to distinguish necessary pubs from gratuitous ones.
#     * Design call ("is this pub shape right?") → the agent's
#       chat-printed PUB GATE verdict block, which the script cannot
#       see by construction. A regex on `^pub ` was approximating
#       this as friction, but produced unfixable false positives on
#       every legitimate new public surface and had no override path.
#   Removing the PUB clause aligns with the gate body's own design
#   (which already delegates mechanical work to zlint and design work
#   to the agent's chat output).
#
# Per-check scope (M70):
#   Unlike the rest of the audit-*.sh family, this script stays
#   diff-shaped (asserts on *added* `^\+` lines, not file state).
#   Each sub-check's rule is shaped as "don't introduce X" rather than
#   "X must not exist anywhere", so a full-codebase scan would change
#   the semantic:
#     * MS-ID — flags milestone identifiers added in this commit.
#               Historical docs in `done/` legitimately contain them,
#               and code comments may reference them in old commits.
#               Only newly-added M{N}_{NNN} citations should fail.
#     * UI    — flags raw HTML primitives added now where a design-system
#               component exists. Legacy raw-HTML in unrelated files is
#               cleaned by the touch-it-fix-it rule (RULE NLR), not by
#               this audit's scope.
#
#   Conversion to full-codebase scope needs separate research. Pre-commit
#   sees staged content through `git diff --cached`, so staged-but-uncommitted
#   content is covered by this script's checks.
#
# Modes:
#   --staged  (default) diff against `git diff --cached -U0`
#             — pre-commit context; index includes staged content
#   --diff              diff against origin/main (vs BASE...HEAD)
#             — used by `make harness-verify-all` periodic deep audit
#
# Each mode also names its POST-IMAGE — the tree the diff's `+` lines belong to
# (the index for --staged, HEAD for --diff). Carve-out recognition reads the
# neighbouring line from there rather than from the diff; see the awk block.
#
# Dispatch façades:
#   dispatch/write_any.md (Milestone-ID Gate)
#   dispatch/write_ts_adhere_bun.md (UI Component Substitution)
#   dispatch/write_zig.md (pub-surface)  (handled by zlint + agent chat output, not this script)

set -euo pipefail

MODE="${1:---staged}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

EXPLICIT=()
case "$MODE" in
  --staged|staged)
    [ "$#" -le 1 ] || { printf 'choose a mode or explicit files\n' >&2; exit 2; }
    DIFF_ARGS=(--cached)
    POST_REF=":"
    LABEL="staged"
    ;;
  --diff|diff)
    [ "$#" -le 1 ] || { printf 'choose a mode or explicit files\n' >&2; exit 2; }
    BASE="${BASE:-origin/main}"
    if ! git rev-parse --verify "${BASE}^{commit}" >/dev/null 2>&1; then
      printf 'FAIL: comparison revision is missing: %s\n' "$BASE" >&2; exit 2
    fi
    DIFF_ARGS=("${BASE}...HEAD")
    POST_REF="HEAD:"
    LABEL="vs $BASE"
    ;;
  -h|--help) printf 'usage: %s [--staged|--diff|file ...]\n' "$0"; exit 0 ;;
  -*) printf 'usage: %s [--staged|--diff|file ...]\n' "$0" >&2; exit 2 ;;
  *)
    for path in "$@"; do
      case "$path" in -*) printf 'unknown audit option: %s\n' "$path" >&2; exit 2 ;; esac
    done
    EXPLICIT=("$@"); POST_REF=""; LABEL="explicit files" ;;
esac

# --staged/--diff: one git diff piped into one awk — the scan itself costs
# two processes however large the staged set, plus at most one `git show`
# per file holding a candidate hit line (zero for a clean diff). Explicit
# mode keeps a per-file scan so the reported path is the caller's own argv
# string (git's --no-index headers strip the leading slash of absolute
# paths, which would misreport the hit and misread the post-image).
# This gate runs inside a 30-minute pre-commit budget that the previous
# per-file fork loop blew through at 1,628 staged files.
#
# The awk learns the current file from the diff's own `+++ b/<path>` header
# instead of a shell loop variable. That parse is made safe on purpose:
#   * `--src-prefix=a/ --dst-prefix=b/` pins the prefixes, because user config
#     (diff.noprefix, diff.mnemonicPrefix) may rename them.
#   * git C-quotes paths containing control characters, `"` or `\` (and with
#     the default core.quotePath, non-ASCII bytes — hence `-c
#     core.quotePath=false` on the git calls), so the header is unquoted in
#     awk. git terminates the header path with one TAB, quoted or not.
#
# Carve-out recognition reads the preceding line from the SOURCE, not from the
# diff. Every carve-out here — the two override markers and the design-system
# `<Section asChild>` wrapper — is a statement about what sits immediately above
# a line in the file, and the diff cannot answer it: `-U0` emits no context
# lines, so the line above an added line appears only when it was itself added.
# Read from the diff, the wrapper carve-out was unfirable for every edit to an
# already-wrapped `<section>` (an aria-label change to one shipped under a
# user-invoked UI GATE override for exactly that reason), and a committed
# override comment stopped covering the line it sits above the moment that line
# was next touched.
#
# The hunk header carries the post-image line number, so `line_above` answers
# from the post-image the mode named. The post-image is slurped once per file —
# on the file's first *candidate* hit line, since only candidates (a line
# matching the MS-ID or UI regexes in a scoped file) ever read the neighbour —
# and dropped when the next file's `diff --git` section opens. One `git show`
# per candidate file, zero for a clean diff, and memory bounded by one file.
scan_diff() {
awk -v post_ref="$POST_REF" '
  # BWK awk (macOS) has no gensub and no --posix quoting help; APOSTROPHE is
  # built rather than written, because this program is inside single quotes.
  # Explicit mode names its file through the environment (see the driver) and
  # the header parse below then never overrides it; the diff modes pass an
  # empty value and every `+++` header re-attributes f.
  BEGIN { APOSTROPHE = sprintf("%c", 39); in_hunk = 0; loaded = 0; imgn = 0
          f = ENVIRON["ORLY_AUDIT_PATH"]; fixed_f = (f != "") }

  function shell_quote(s,    i,c,out) {
    out = APOSTROPHE
    for (i = 1; i <= length(s); i++) {
      c = substr(s, i, 1)
      if (c == APOSTROPHE) out = out APOSTROPHE "\\" APOSTROPHE APOSTROPHE
      else out = out c
    }
    return out APOSTROPHE
  }

  # The post-image path a `+++ ` header names: C-quote resolved (git wraps
  # the whole "b/<path>" in quotes), the pinned b/ prefix removed. /dev/null
  # maps to the empty path, which matches no scope below.
  function dst_path(header,    rest,out,i,c,nxt,q,oct,v,j) {
    rest = substr(header, 5)
    if (rest == "/dev/null") return ""
    if (substr(rest, 1, 1) == "\"") {
      q = 0
      for (i = length(rest); i >= 2; i--) if (substr(rest, i, 1) == "\"") { q = i; break }
      rest = substr(rest, 2, q - 2)
      out = ""
      i = 1
      while (i <= length(rest)) {
        c = substr(rest, i, 1)
        if (c == "\\") {
          nxt = substr(rest, i + 1, 1)
          if (nxt ~ /[0-7]/) {
            # \ooo octal escape, one to three digits (only control bytes reach
            # here: core.quotePath=false passes non-ASCII through raw).
            oct = nxt
            while (length(oct) < 3 && substr(rest, i + 1 + length(oct), 1) ~ /[0-7]/)
              oct = oct substr(rest, i + 1 + length(oct), 1)
            v = 0
            for (j = 1; j <= length(oct); j++) v = v * 8 + substr(oct, j, 1)
            out = out sprintf("%c", v)
            i += 1 + length(oct)
          } else {
            if (nxt == "a") out = out sprintf("%c", 7)
            else if (nxt == "b") out = out sprintf("%c", 8)
            else if (nxt == "f") out = out "\f"
            else if (nxt == "n") out = out "\n"
            else if (nxt == "r") out = out "\r"
            else if (nxt == "t") out = out "\t"
            else if (nxt == "v") out = out "\v"
            else out = out nxt
            i += 2
          }
        } else { out = out c; i++ }
      }
      rest = out
    } else {
      # Unquoted: drop the single trailing TAB git appends to the header
      # path. A literal tab in a filename forces C-quoting, so a trailing
      # tab here is never name content; a trailing SPACE is, and is kept.
      sub(/\t$/, "", rest)
    }
    sub(/^b\//, "", rest)
    return rest
  }

  function load_postimage(    cmd,l) {
    imgn = 0
    if (post_ref == "") {
      while ((getline l < f) > 0) { imgn++; img[imgn] = l }
      close(f)
    } else {
      cmd = "git show " shell_quote(post_ref f) " 2>/dev/null"
      while ((cmd | getline l) > 0) { imgn++; img[imgn] = l }
      close(cmd)
    }
    loaded = 1
  }

  # The source line above post-image line `n` of the current file, or "" when
  # there is none. Reads the slurped post-image; the slurp happens at most
  # once per file section.
  function line_above(n) {
    if (n <= 1) return ""
    if (!loaded) load_postimage()
    if (n - 1 > imgn) return ""
    return img[n - 1]
  }

  # A new file section: reset hunk state and drop the previous post-image —
  # lookups only ever concern the file of the open section.
  /^diff --git / {
    in_hunk = 0
    if (loaded) { for (i = 1; i <= imgn; i++) delete img[i]; imgn = 0; loaded = 0 }
    next
  }
  !in_hunk && /^\+\+\+ / { if (!fixed_f) f = dst_path($0); next }
  /^\+\+\+/ { next }
  # `@@ -a,b +c,d @@` — $3 is the post-image start line for this hunk.
  /^@@/ { in_hunk = 1; start=$3; sub(/^\+/,"",start); split(start,parts,","); nl=parts[1]+0; next }
  /^[^+]/ { next }
  /^\+/ {
    line=$0
    sub(/^\+/,"",line)
    # The line above is fetched only for candidate hits: every reader of
    # `prev` sits behind one of these two guards, so a line that matches
    # neither regex never pays for the post-image.
    msid_cand = f ~ /\.(zig|sql|ts|tsx|js|jsx|py|rs|go|sh|toml|yaml|json)$/ &&
                f !~ /^(docs|node_modules|vendor|third_party)\// &&
                match(line, /M[0-9]+_[0-9]+|§[0-9]+(\.[0-9]+)+|(^|[^A-Za-z0-9_])T[0-9]+([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_])dim [0-9]+\.[0-9]+([^A-Za-z0-9_]|$)/)
    ui_cand = f ~ /^ui\/packages\/app\/.*\.(tsx|jsx)$/ &&
              line ~ /<(section|button|input|dialog|article|nav|header|form)[ \t>\/]/
    if (msid_cand || ui_cand) {
      prev = line_above(nl)
      ms_id_override = (prev ~ /MILESTONE ID ALLOWED per user override/)
      ui_override = (prev ~ /UI GATE: SKIPPED per user override/)
    } else {
      prev = ""; ms_id_override = 0; ui_override = 0
    }
    nl++
    if (msid_cand && !ms_id_override) {
      print "MS-ID  " f ": " line
    }
    # react-hook-form caveat: its prescribed shape is a raw
    # <form onSubmit={form.handleSubmit(...)}> inside the design-system <Form>
    # provider (see design-system Form.tsx docstring). No DS form-element
    # primitive exists to substitute to, so the substitute-if-a-primitive-exists
    # rule leaves the RHF <form> as the correct raw element. Exempt exactly that
    # shape; a bare <form> or <form action=...> with no handleSubmit still trips.
    rhf_form = (line ~ /<form[ \t]/ && line ~ /handleSubmit/)
    # <section> landmark caveat: a semantic <section aria-label> region has no DS
    # primitive (Section renders a <div>; role="region" trips oxlint
    # jsx-a11y/prefer-tag-over-role, which mandates the raw tag). The DS pattern
    # wraps the raw tag in the asChild form of the DS Section, so exempt a
    # <section> whose preceding SOURCE line opens that wrapper — an edit to an
    # already-wrapped section changes only the <section> line, and the wrapper
    # is above it in the file whether or not this diff touched it. A
    # bare/unwrapped <section> still trips.
    ds_section = (line ~ /<section[ \t]/ && prev ~ /<Section asChild>/)
    if (ui_cand && !ui_override && !rhf_form && !ds_section) {
      print "UI     " f ": " line
    }
  }
'
}

hits=$(
  if [ "${#EXPLICIT[@]}" -gt 0 ]; then
    for path in "${EXPLICIT[@]}"; do
      [ -f "$path" ] || continue
      # diff returns one for differences; that is expected input here.
      { git -c core.quotePath=false diff --no-index --no-ext-diff -U0 --src-prefix=a/ --dst-prefix=b/ -- /dev/null "$path" || [ "$?" -eq 1 ]; } | ORLY_AUDIT_PATH="$path" scan_diff
    done
  else
    # --no-renames pins a rename to its delete+add shape: this gate scores
    # added lines, and its verdicts are defined over renamed files being
    # re-scanned as whole-file additions. Rename detection would silently
    # stop scanning their contents.
    git -c core.quotePath=false diff --no-ext-diff -U0 --no-renames --diff-filter=ACMRT --src-prefix=a/ --dst-prefix=b/ "${DIFF_ARGS[@]}" | ORLY_AUDIT_PATH="" scan_diff
  fi
)

if [ -n "$hits" ]; then
  echo "FAIL: audit-msid-ui ($LABEL) — MS-ID / UI hits below"
  printf '%s\n' "$hits"
  echo
  echo "Resolve each hit OR carve out via the relevant gate's override comment."
  echo "  MS-ID — strip the marker; production code must be milestone-free (RULE TST-NAM)"
  echo "  UI    — use the design-system primitive; carve out with 'UI GATE: SKIPPED'"
  exit 1
fi

echo "OK:   audit-msid-ui ($LABEL) — 0 hits"
