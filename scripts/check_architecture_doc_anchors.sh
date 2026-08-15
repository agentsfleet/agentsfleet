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
#
# A section with no link ahead of it on the line names a heading on its own
# page, and comes back as `@self`. Those are the majority and they are why the
# quoting rule matters: `§C. EXECUTE step 3` and `§Scope catalogue (meaning)`
# read fine to a person and parse as nothing, so they went unchecked for as
# long as they have existed. A same-page reference stays unlinked — quoting is
# the whole fix. A reference to another page has to carry a link, because
# nothing else says which page it means.

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
    function scan(line,   rest, tok, target) {
      rest = line
      target = ""
      while (match(rest, PAT)) {
        gap  = substr(rest, 1, RSTART - 1)
        tok  = substr(rest, RSTART, RLENGTH)
        rest = substr(rest, RSTART + RLENGTH)
        # A link binds the sections that follow it in the same sentence, and no
        # further. Past a full stop or a table-cell divider the subject has
        # changed, so a bare section there is about the current page again —
        # "(see other.md §\"X\"). The figure in §\"Y\" was wrong" names two
        # different pages, and binding to the nearest link gets the second wrong.
        if (gap ~ /[.][[:space:]]/ || index(gap, "|") > 0) target = ""
        if (substr(tok, 1, 1) == "]") {        # a link on its own: sets context
          target = pathof(tok)
        } else if (index(tok, "](") > 0) {     # link and section together
          target = pathof(tok)
          print src "::" target "::" anchorof(tok)
        } else {                               # a section against the last link,
          # or, with no link ahead of it on the line, against its own page.
          print src "::" (target == "" ? "@self" : target) "::" anchorof(tok)
        }
      }
    }
    # Prose is hard-wrapped, so a quoted anchor can straddle the line break and
    # arrive at the scanner cut in half. An odd number of quotes on a line that
    # opens one is the signal; hold it and read on. The cap stops a stray quote
    # from swallowing the rest of the file.
    function open_quote(s) { return (index(s, "§\"") > 0 && gsub(/"/, "\"", s) % 2 == 1) }
    {
      line = (held == "" ? $0 : held " " $0)
      if (open_quote(line) && holds < 3) { held = line; holds++; next }
      held = ""; holds = 0
      scan(line)
    }
    END { if (held != "") scan(held) }
  ' "$f" | sort -u
done
