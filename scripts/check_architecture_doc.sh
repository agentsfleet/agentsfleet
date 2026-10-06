#!/usr/bin/env bash
# Architecture doc consistency gate. Run via `make check-architecture-doc`
# (a prerequisite of `make lint-all`), or directly:
#
#     bash scripts/check_architecture_doc.sh
#
# Tests covered:
#   * test_arch_M_references_resolve     — every milestone identifier resolves
#   * test_arch_anchor_links_resolve     — every relative .md link target exists
#   * test_arch_no_orphan_TODO           — 0 TODO/TKTK/FIXME hits in architecture/
#   * architecture_schedule_ownership    — cron ownership names QStash, not NullClaw
#   * test_arch_cited_paths_resolve      — every cited source path is a tracked file
#   * test_arch_cited_tables_exist       — every named table is defined in schema/
#   * test_arch_cited_make_targets_exist — every named make target is declared
#   * test_arch_section_anchors_resolve  — every cross-page §anchor names a heading
#   * test_arch_no_retired_slot_numbers  — no page cites a retired 0xx schema slot
#
# ARCH_DIR, SPEC_ROOT and DOC_SET_EXTRA are overridable so the self-tests can
# point the gate at fixtures. Nothing else sets them.
#
# Exits 0 on success, 1 on the first failing assertion (with diagnostic).

set -euo pipefail

# Resolved from BASH_SOURCE, not `$PWD`: the gate runs from the repository root
# but the self-tests invoke it by absolute path, and its sibling extractor has to
# resolve either way.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ARCH_DIR="${ARCH_DIR:-docs/architecture}"
SPEC_ROOT="${SPEC_ROOT:-docs/v2}"
DONE_DIR="$SPEC_ROOT/done"
ACTIVE_DIR="$SPEC_ROOT/active"


FAIL=0

err() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()  { printf "OK:   %s\n" "$*"; }

# A missing ARCH_DIR must be a hard error, not a vacuous pass: without this, a
# standalone run against a moved or renamed docs tree reports green while checking
# nothing (every scan below is guarded with `2>/dev/null` and would find zero).
if [ ! -d "$ARCH_DIR" ]; then
  err "ARCH_DIR '$ARCH_DIR' is not a directory — nothing to check (moved corpus?)"
  exit "$FAIL"
fi

# ---------------------------------------------------------------------------
# 1. test_arch_M_references_resolve
#    Every milestone identifier in architecture/ must resolve to a spec in done/
#    (shipped) or active/ (in flight, e.g. the spec doing the cross-ref itself).
#    An identifier with no spec anywhere fails. A `pending/` spec is not evidence
#    that work exists: the page that traded on that exemption is gone.
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
      err "test_arch_M_references_resolve: $ref cited in $src resolves to no spec in $DONE_DIR/ or $ACTIVE_DIR/"
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
# 4. architecture_schedule_ownership
# ---------------------------------------------------------------------------
if [ -f "$ARCH_DIR/data_flow.md" ] && [ -f "$ARCH_DIR/user_flow.md" ] && [ -f "$ARCH_DIR/high_level.md" ] && [ -f "$ARCH_DIR/README.md" ]; then
  if ! grep -q "QStash owns the clock" "$ARCH_DIR/data_flow.md"; then
    err "architecture_schedule_ownership: data_flow.md must state QStash owns the clock"
  fi
  if ! grep -q "QStash owns the clock" "$ARCH_DIR/user_flow.md"; then
    err "architecture_schedule_ownership: user_flow.md must state QStash owns the clock"
  fi
  if ! grep -q "synchronously registered with Upstash QStash" "$ARCH_DIR/high_level.md"; then
    err "architecture_schedule_ownership: high_level.md must name Upstash QStash as the cron provider"
  fi
  if ! grep -q "Upstash QStash" "$ARCH_DIR/README.md"; then
    err "architecture_schedule_ownership: README.md must define cron trigger ownership"
  fi
  # The two sentences that shipped when the NullClaw child kept its own timer,
  # matched exactly. A looser pattern cannot tell them from the true design:
  # `cron_add` is a schedule tool on the daemon's plane, and data_flow.md says in
  # the same words that no NullClaw child owns a schedule timer. The four
  # assertions above pin what IS true; this list names only what was false.
  stale_schedule_hits=$(grep -rnF -e "NullClaw-managed schedule" -e "NullClaw's \`cron_add\`" "$ARCH_DIR" 2>/dev/null || true)
  if [ -n "$stale_schedule_hits" ]; then
    err "architecture_schedule_ownership: stale local-scheduler ownership text found:"
    printf "%s\n" "$stale_schedule_hits" >&2
  fi
  [ "$FAIL" = 0 ] && ok "architecture_schedule_ownership: QStash/agentsfleetd ownership is consistent"
fi

# ---------------------------------------------------------------------------
# 5. architecture_absent_mechanisms
#
# Two mechanisms the pages described for a year and the daemon has never had.
# Both read as current, which is the whole problem: a reader budgets for a
# guarantee that is not there.
#
#   Row-Level Security. `schema/` declares no `ROW LEVEL SECURITY` and no
#   policy, and nothing reads a `current_setting('app.workspace_id')`. Workspace
#   isolation is enforced in the application, by the ownership layer in front of
#   every workspace route.
#
#   The session execution handle. `core.fleet_sessions.execution_id` has no
#   writer and no reader in the Rust tree; `fleet.runner_leases` answers "which
#   fleet is executing" with a fencing token and an expiry behind it.
#
# Asserted as ABSENCE of the claim rather than presence of a correction,
# because there is no one sentence a page must carry — only a thing it must
# stop saying. The `grep -v` skips the rows that name the absence on purpose.
# ---------------------------------------------------------------------------
if [ -f "$ARCH_DIR/data_flow.md" ]; then
  rls_claims=$(grep -rn "Row-Level Security\|Row Level Security" "$ARCH_DIR" 2>/dev/null \
    | grep -v "declares no" || true)
  if [ -n "$rls_claims" ]; then
    err "architecture_absent_mechanisms: this repository declares no row-level security policy:"
    printf "%s\n" "$rls_claims" >&2
  fi
  handle_claims=$(grep -rn "execution_id" "$ARCH_DIR" 2>/dev/null \
    | grep -v "nothing writes or reads either" || true)
  if [ -n "$handle_claims" ]; then
    err "architecture_absent_mechanisms: the session execution handle has no writer or reader:"
    printf "%s\n" "$handle_claims" >&2
  fi
  [ "$FAIL" = 0 ] && ok "architecture_absent_mechanisms: no page claims a mechanism the daemon lacks"
fi

# ---------------------------------------------------------------------------
# Citation assertions live beside this file and run in this shell, sharing
# `err`, `ok`, `FAIL`, `ARCH_DIR` and `SCRIPT_DIR`. Split at the length cap.
# ---------------------------------------------------------------------------
# shellcheck source=scripts/check_architecture_doc_citations.sh
. "$SCRIPT_DIR/check_architecture_doc_citations.sh"

# ---------------------------------------------------------------------------
exit "$FAIL"
