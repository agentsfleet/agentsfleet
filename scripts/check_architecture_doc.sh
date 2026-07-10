#!/usr/bin/env bash
# Architecture doc consistency gate. Run via `make check-architecture-doc`
# (a prerequisite of `make lint-all`), or directly:
#
#     bash scripts/check_architecture_doc.sh
#
# Tests covered:
#   * test_arch_M_references_resolve  — every milestone identifier resolves
#   * test_arch_anchor_links_resolve  — every relative .md link target exists
#   * test_arch_no_orphan_TODO        — 0 TODO/TKTK/FIXME hits in architecture/
#
# ARCH_DIR and SPEC_ROOT are overridable so check_architecture_doc_test.sh can
# point the gate at fixtures. Nothing else sets them.
#
# Exits 0 on success, 1 on the first failing assertion (with diagnostic).

set -euo pipefail

ARCH_DIR="${ARCH_DIR:-docs/architecture}"
SPEC_ROOT="${SPEC_ROOT:-docs/v2}"
DONE_DIR="$SPEC_ROOT/done"
ACTIVE_DIR="$SPEC_ROOT/active"
PENDING_DIR="$SPEC_ROOT/pending"

# The single architecture doc whose subject is unshipped work. Everywhere else a
# milestone reference asserts a fact about the system, so it must name a spec
# that shipped (done/) or is in flight (active/); the roadmap names what is
# merely planned, and a pending/ spec is the only evidence such work exists.
readonly ROADMAP_BASENAME="roadmap.md"

FAIL=0

err() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()  { printf "OK:   %s\n" "$*"; }

# ---------------------------------------------------------------------------
# 1. test_arch_M_references_resolve
#    Every milestone identifier in architecture/ must resolve to a spec in done/
#    (shipped) or active/ (in flight, e.g. the spec doing the cross-ref itself).
#    pending/ resolves in roadmap.md alone — see ROADMAP_BASENAME above. An
#    identifier with no spec anywhere fails in every file, roadmap included.
# ---------------------------------------------------------------------------

# True when some `<base>_*.md` spec lives in `dir`.
spec_exists() {
  ls "$1/$2"_*.md >/dev/null 2>&1
}

# `src_file` decides whether pending/ counts; `ref` may carry a workstream suffix,
# which the milestone glob strips before matching a spec filename.
resolve_ref() {
  local src_file="$1"
  local base="${2%%_*}"

  if spec_exists "$DONE_DIR" "$base"; then return 0; fi
  if spec_exists "$ACTIVE_DIR" "$base"; then return 0; fi
  if [ "$(basename "$src_file")" = "$ROADMAP_BASENAME" ] && spec_exists "$PENDING_DIR" "$base"; then
    return 0
  fi
  return 1
}

# `file:REF` pairs, not bare refs: which file cited an identifier decides whether
# pending/ resolves it, so the filename has to survive the scan.
m_refs=$(grep -rEo "M[0-9]+_[0-9]+|\bM[0-9]+\b" "$ARCH_DIR" 2>/dev/null | sort -u || true)

if [ -z "$m_refs" ]; then
  ok "no milestone references in $ARCH_DIR/ (vacuously resolves)"
else
  m_count=0
  # Here-doc, not a pipe: a `while read` on the right of a pipe runs in a subshell
  # and every err() would set FAIL in a shell that exits before the check reads it.
  while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    src="${entry%%:*}"
    ref="${entry##*:}"
    if resolve_ref "$src" "$ref"; then
      m_count=$((m_count + 1))
    else
      err "test_arch_M_references_resolve: $ref cited in $src resolves to no spec in $DONE_DIR/ or $ACTIVE_DIR/ (pending/ resolves only in $ROADMAP_BASENAME)"
    fi
  done <<EOF
$m_refs
EOF
  [ "$FAIL" = 0 ] && ok "test_arch_M_references_resolve: all $m_count milestone references resolve"
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
# 3. test_arch_no_orphan_TODO
# ---------------------------------------------------------------------------
todo_hits=$(grep -rEn "TODO|TKTK|FIXME" "$ARCH_DIR" 2>/dev/null || true)
if [ -n "$todo_hits" ]; then
  err "test_arch_no_orphan_TODO: orphan markers found in architecture/:"
  printf "%s\n" "$todo_hits" >&2
else
  ok "test_arch_no_orphan_TODO: no TODO/TKTK/FIXME in architecture/"
fi

# ---------------------------------------------------------------------------
exit "$FAIL"
