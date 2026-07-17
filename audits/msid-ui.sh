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
    LABEL="staged"
    ;;
  --diff|diff)
    BASE="${BASE:-origin/main}"
    if ! git rev-parse --verify "$BASE" >/dev/null 2>&1; then BASE="HEAD"; fi
    DIFF_CMD="git diff -U0 ${BASE}...HEAD"
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
# Override-recognition: an immediately-preceding `+` line in the diff
# containing the relevant override marker suppresses the next hit for
# that rule on the current `+` line.
hits=$($DIFF_CMD | awk '
  /^\+\+\+ b\// { f=$2; sub("^b/","",f); prev_added=""; next }
  /^[^+]/ { prev_added=""; next }
  /^\+/ {
    line=$0
    sub(/^\+/,"",line)
    ms_id_override = (prev_added ~ /MILESTONE ID ALLOWED per user override/)
    ui_override = (prev_added ~ /UI GATE: SKIPPED per user override/)
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
    # is <Section asChild><section …>, so exempt a <section> whose immediately
    # preceding added line opens <Section asChild>. A bare/unwrapped <section>
    # still trips.
    ds_section = (line ~ /<section[ \t]/ && prev_added ~ /<Section asChild>/)
    if (!ui_override && !rhf_form && !ds_section &&
        f ~ /^ui\/packages\/app\/.*\.(tsx|jsx)$/ &&
        line ~ /<(section|button|input|dialog|article|nav|header|form)[ \t>\/]/) {
      print "UI     " f ": " line
    }
    prev_added = line
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
