#!/usr/bin/env bash
# Cross-page section-pointer extraction for check_architecture_doc.sh (§8).
#
# Reads doc paths on stdin, writes one `src::target::anchor` triple per pointer
# to stdout. It lives beside the gate rather than inside it so the pointer
# spellings have one home, and so both files stay inside the 350-line cap.
#
# A pointer is a link plus a section. The section can sit after the link —
# `[`page.md`](./page.md) §Section` — or inside its text —
# `[`page.md` §Section](./page.md)` — and one link can carry several sections:
# `[`page.md`](./page.md) §"B. TRIGGER" and §"C. EXECUTE"`. So this is a
# left-to-right scan per line, not a set of independent greps: a link sets the
# current target, and each section that follows resolves against it. Greps
# keyed on the link alone read the first section of a line and miss the rest.
#
# Quoting is what carries punctuation. A quoted anchor runs to the closing
# quote, so `§"C. EXECUTE"` survives whole; unquoted stops at the first comma,
# semicolon, pipe, paren or bracket. Brackets end it because a bare anchor sits
# next to links it must not swallow — `§Flow 1 + [`page.md`](./page.md)` names
# a section on the current page and a different page after it, not one pointer.
# An anchor needing any of those characters must be quoted: the gate matches on
# a heading's opening words, so a truncated anchor is what silently matches the
# wrong heading. The gate rejects an anchor matching more than one heading,
# which is how a truncated-to-ambiguous one gets caught rather than passing.
#
# The target keeps its `./` or `../` prefix: the gate joins it to the source's
# directory, and dropping `../` resolved a sibling-directory pointer to a path
# that does not exist, which the gate skipped rather than checked.

set -euo pipefail

while IFS= read -r f; do
  [ -f "$f" ] || continue
  awk -v src="$f" '
    BEGIN {
      # Ordered most-specific first: an inside-link pointer starts at the `[`
      # that opens the link text, so it wins over the bare link on the left.
      LINK = "\\]\\(\\.\\.?/[^)]*\\.md\\)"
      PAT  = "\\[[^]]*§\"[^\"]+\"" LINK \
             "|\\[[^]]*§[A-Za-z][^]\"]*" LINK \
             "|" LINK \
             "|§\"[^\"]+\"" \
             "|§[A-Za-z][^],;|()\"[]*"
    }
    function pathof(tok,   p) {
      p = substr(tok, index(tok, "](") + 2)
      sub(/\)$/, "", p)
      return p
    }
    function anchorof(tok,   a, i) {
      a = tok
      sub(/\][(][^)]*[)]$/, "", a)   # an inside-link anchor sheds its link
      i = index(a, "§")
      a = substr(a, i + length("§"))
      sub(/^[[:space:]]+/, "", a)
      sub(/[[:space:]]*\.?[[:space:]]*$/, "", a)
      gsub(/^"|"$/, "", a)
      return a
    }
    {
      rest = $0
      target = ""
      while (match(rest, PAT)) {
        tok  = substr(rest, RSTART, RLENGTH)
        rest = substr(rest, RSTART + RLENGTH)
        if (substr(tok, 1, 1) == "]") {        # a link on its own: sets context
          target = pathof(tok)
        } else if (index(tok, "](") > 0) {     # link and section together
          target = pathof(tok)
          print src "::" target "::" anchorof(tok)
        } else if (target != "") {             # a section against the last link
          print src "::" target "::" anchorof(tok)
        }
      }
    }
  ' "$f" | sort -u
done
