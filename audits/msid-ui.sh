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

case "$MODE" in
  --staged|staged)
    DIFF_CMD="git diff --cached -U0"
    POST_REF=":"
    LABEL="staged"
    ;;
  --diff|diff)
    BASE="${BASE:-origin/main}"
    if ! git rev-parse --verify "$BASE" >/dev/null 2>&1; then BASE="HEAD"; fi
    DIFF_CMD="git diff -U0 ${BASE}...HEAD"
    POST_REF="HEAD:"
    LABEL="vs $BASE"
    ;;
  --all|all)
    # No "all" mode — this audit is diff-shaped by construction (it
    # asserts on *added* lines, not file state). Force callers to pick.
    echo "usage: $0 [--staged|--diff]" >&2
    exit 2
    ;;
  *)
    echo "usage: $0 [--staged|--diff]" >&2; exit 2 ;;
esac

# Single awk pass; reads stdin (the unified diff).
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
# The hunk header carries the post-image line number, so `line_above` can fetch
# the neighbouring line out of the post-image the mode named. Same shape as
# audits/design-tokens.sh, which has always read its override out of the file.
hits=$($DIFF_CMD | awk -v post_ref="$POST_REF" '
  # BWK awk (macOS) has no gensub and no --posix quoting help; APOSTROPHE is
  # built rather than written, because this program is inside single quotes.
  BEGIN { APOSTROPHE = sprintf("%c", 39) }

  # The source line above line `n` of `path`, from the post-image, or "" when
  # there is none. Cached: a hunk may ask about the same neighbour repeatedly,
  # and each miss costs a fork.
  function line_above(path, n,    key, cmd, out) {
    if (n <= 1) return ""
    key = path SUBSEP n
    if (key in above) return above[key]
    above[key] = ""
    # A path holding an apostrophe cannot go through the quoting below. No
    # carve-out beats a mis-quoted shell command built from a filename.
    if (index(path, APOSTROPHE) > 0) return ""
    cmd = "git show " APOSTROPHE post_ref path APOSTROPHE " 2>/dev/null | sed -n " (n - 1) "p"
    if ((cmd | getline out) > 0) above[key] = out
    close(cmd)
    return above[key]
  }

  /^\+\+\+ b\// { f=$2; sub("^b/","",f); next }
  # `@@ -a,b +c,d @@` — $3 is the post-image start line for this hunk.
  /^@@/ { start=$3; sub(/^\+/,"",start); split(start,parts,","); nl=parts[1]+0; next }
  /^[^+]/ { next }
  /^\+/ {
    line=$0
    sub(/^\+/,"",line)
    prev = line_above(f, nl)
    nl++
    ms_id_override = (prev ~ /MILESTONE ID ALLOWED per user override/)
    ui_override = (prev ~ /UI GATE: SKIPPED per user override/)
    if (!ms_id_override &&
        f ~ /\.(zig|sql|ts|tsx|js|jsx|py|rs|go|sh|toml|yaml|json)$/ &&
        f !~ /^(docs|node_modules|vendor|third_party)\//) {
      if (match(line, /M[0-9]+_[0-9]+|§[0-9]+(\.[0-9]+)+|(^|[^A-Za-z0-9_])T[0-9]+([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_])dim [0-9]+\.[0-9]+([^A-Za-z0-9_]|$)/)) {
        print "MS-ID  " f ": " line
      }
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
    if (!ui_override && !rhf_form && !ds_section &&
        f ~ /^ui\/packages\/app\/.*\.(tsx|jsx)$/ &&
        line ~ /<(section|button|input|dialog|article|nav|header|form)[ \t>\/]/) {
      print "UI     " f ": " line
    }
  }
')

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
