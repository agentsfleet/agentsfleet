#!/usr/bin/env bash
# Architecture doc consistency tests required by docs/v2/active/M51_001
# Test Specification (folded from M50). Run via:
#
#     bash scripts/check_architecture_doc.sh
#
# Tests covered:
#   * test_arch_M_references_resolve  — every M{N} mentioned has a done/ spec
#   * test_arch_anchor_links_resolve  — every relative .md link target exists
#   * test_arch_section_14_present    — §14 ship_reflection.md exists, non-empty,
#                                        under 600 words
#   * test_arch_no_orphan_TODO        — 0 TODO/TKTK/FIXME hits in architecture/
#
# Exits 0 on success, 1 on the first failing assertion (with diagnostic).

set -euo pipefail

ARCH_DIR="docs/architecture"
DONE_DIR="docs/v2/done"
FAIL=0

err() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()  { printf "OK:   %s\n" "$*"; }

# ---------------------------------------------------------------------------
# 1. test_arch_M_references_resolve
#    Each M{N} in architecture/ must resolve to a spec in done/ (shipped) or
#    active/ (currently in-flight, e.g. the spec doing the cross-ref itself).
#    pending/ is NOT acceptable — pending specs are aspirational, not load-bearing.
# ---------------------------------------------------------------------------
m_ids=$(grep -rEoh "M[0-9]+_[0-9]+|\bM[0-9]+\b" "$ARCH_DIR" 2>/dev/null \
  | grep -E "^M(40|41|42|43|44|45|46|47|48|49|50|51)" \
  | sort -u || true)

if [ -z "$m_ids" ]; then
  ok "no M{N} references in architecture/ (vacuously resolves)"
else
  m_count=0
  for ref in $m_ids; do
    # Strip subspec suffix (e.g. M45_001 -> M45) for prefix glob match
    base="${ref%%_*}"
    if ls "$DONE_DIR"/"${base}"_*.md >/dev/null 2>&1; then
      m_count=$((m_count + 1))
    elif ls "docs/v2/active/${base}"_*.md >/dev/null 2>&1; then
      m_count=$((m_count + 1))
    else
      err "test_arch_M_references_resolve: $ref referenced in architecture/ but no $DONE_DIR/${base}_*.md or docs/v2/active/${base}_*.md found"
    fi
  done
  [ "$FAIL" = 0 ] && ok "test_arch_M_references_resolve: all $m_count M-IDs resolve to done/ or active/ specs"
fi

# ---------------------------------------------------------------------------
# 2. test_arch_anchor_links_resolve  (relative .md file links)
# ---------------------------------------------------------------------------
# Captures `](./foo.md)` and `](../foo.md)` style links. Skips http(s):// links.
broken_links=0
while IFS= read -r entry; do
  src_file="${entry%%::*}"
  link="${entry##*::}"
  src_dir=$(dirname "$src_file")
  # Strip trailing #anchor for file existence check
  rel_path="${link%%#*}"
  resolved=$(cd "$src_dir" && pwd)/"$rel_path"
  resolved_norm=$(cd "$(dirname "$resolved")" 2>/dev/null && pwd)/"$(basename "$resolved")" || true
  if [ ! -f "$resolved_norm" ]; then
    err "test_arch_anchor_links_resolve: $src_file → $link (resolved: $resolved_norm) does not exist"
    broken_links=$((broken_links + 1))
  fi
done < <(grep -rEon '\]\(\.\.?/[^)]+\.md[^)]*\)' "$ARCH_DIR" 2>/dev/null \
  | sed -E 's|^([^:]+):[0-9]+:.*\]\((\.\.?/[^)]+)\)|\1::\2|' || true)

[ "$broken_links" = 0 ] && ok "test_arch_anchor_links_resolve: all relative .md links resolve"

# ---------------------------------------------------------------------------
# 3. test_arch_section_14_present
# ---------------------------------------------------------------------------
SR_FILE="$ARCH_DIR/ship_reflection.md"
if [ ! -f "$SR_FILE" ]; then
  err "test_arch_section_14_present: $SR_FILE missing"
elif ! head -3 "$SR_FILE" | grep -qE "^# 14\."; then
  err "test_arch_section_14_present: $SR_FILE does not start with '# 14.' header"
else
  word_count=$(wc -w < "$SR_FILE" | tr -d ' ')
  if [ "$word_count" -gt 600 ]; then
    err "test_arch_section_14_present: $SR_FILE is $word_count words (cap is 600 per spec §4.3)"
  elif [ "$word_count" -lt 50 ]; then
    err "test_arch_section_14_present: $SR_FILE is only $word_count words (looks empty/stub-only)"
  else
    ok "test_arch_section_14_present: $SR_FILE present, $word_count words (cap 600)"
  fi
fi

# ---------------------------------------------------------------------------
# 4. test_arch_no_orphan_TODO
# ---------------------------------------------------------------------------
# `PENDING SHIP` markers in ship_reflection.md are deliberate, not orphans.
todo_hits=$(grep -rEn "TODO|TKTK|FIXME" "$ARCH_DIR" 2>/dev/null || true)
if [ -n "$todo_hits" ]; then
  err "test_arch_no_orphan_TODO: orphan markers found in architecture/:"
  printf "%s\n" "$todo_hits" >&2
else
  ok "test_arch_no_orphan_TODO: no TODO/TKTK/FIXME in architecture/"
fi

# ---------------------------------------------------------------------------
exit "$FAIL"
