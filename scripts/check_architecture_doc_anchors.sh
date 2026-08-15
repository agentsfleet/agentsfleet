#!/usr/bin/env bash
# Cross-page section-pointer extraction for check_architecture_doc.sh (§8).
#
# Reads doc paths on stdin, writes one `src::target.md::anchor` triple per
# pointer to stdout. It lives beside the gate rather than inside it so the
# pointer spellings have one home, and so both files stay inside the 350-line
# cap (RULE FLL).
#
# Four spellings are live in the corpus, and all four are read. The section can
# sit AFTER the link — `[`page.md`](./page.md) §Section` — or INSIDE its text —
# `[`page.md` §Section](./page.md)`. Either position can quote the anchor.
#
# Quoting is what carries punctuation: a quoted anchor runs to the closing
# quote, so `§"C. EXECUTE"` survives whole. The unquoted form stops at the first
# comma, semicolon, pipe, paren or closing bracket, and drops a sentence-final
# period. An anchor that needs any of those characters must be quoted; the gate
# matches on the heading's opening words, so a truncated anchor is what silently
# points at the wrong heading.

set -euo pipefail

# The link destination, matched once and captured once. Both spellings share it.
readonly LINK='\]\(\.\.?/[A-Za-z0-9_/-]+\.md\)'
readonly LINK_CAP='\]\(\.\.?/([A-Za-z0-9_/-]+\.md)\)'

while IFS= read -r f; do
  [ -f "$f" ] || continue
  {
    # After the link, quoted.
    grep -oE "${LINK}[[:space:]]*§\"[^\"]+\"" "$f" 2>/dev/null \
      | sed -E "s|${LINK_CAP}[[:space:]]*§\"([^\"]+)\"|::\1::\2|" || true
    # After the link, unquoted. `[A-Za-z]` skips a numeric anchor (`§4`), which
    # names a heading the page numbers itself rather than repeating in its text.
    grep -oE "${LINK}[[:space:]]*§[A-Za-z][^,;|)\"]*" "$f" 2>/dev/null \
      | sed -E "s|${LINK_CAP}[[:space:]]*§|::\1::|" || true
    # Inside the link text, quoted. The anchor precedes the destination here, so
    # the capture groups come out reversed and the replacement swaps them back.
    grep -oE "\[[^]]*§\"[^\"]+\"${LINK}" "$f" 2>/dev/null \
      | sed -E "s|\[[^]]*§\"([^\"]+)\"${LINK_CAP}|::\2::\1|" || true
    # Inside the link text, unquoted. Link text cannot contain `]`, so the
    # closing bracket terminates the anchor on its own.
    grep -oE "\[[^]]*§[A-Za-z][^],;|\"]*${LINK}" "$f" 2>/dev/null \
      | sed -E "s|\[[^]]*§([^]]*)${LINK_CAP}|::\2::\1|" || true
  } | sed -E 's/[[:space:]]*\.?[[:space:]]*$//' | sort -u | sed "s|^|$f|" || true
done
