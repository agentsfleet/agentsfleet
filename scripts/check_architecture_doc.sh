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
#
# ARCH_DIR, SPEC_ROOT and DOC_SET_EXTRA are overridable so
# check_architecture_doc_test.sh can point the gate at fixtures. Nothing else
# sets them.
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
PENDING_DIR="$SPEC_ROOT/pending"

# The single architecture doc whose subject is unshipped work. Everywhere else a
# milestone reference asserts a fact about the system, so it must name a spec
# that shipped (done/) or is in flight (active/); the roadmap names what is
# merely planned, and a pending/ spec is the only evidence such work exists.
# The carve-out matches this exact path, not the basename — a nested
# `scenarios/roadmap.md` must not inherit the exemption and launder unshipped ids.
readonly ROADMAP_REL_PATH="roadmap.md"

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
#    pending/ resolves in roadmap.md alone — see ROADMAP_REL_PATH above. An
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
  if [ "${src_file#"$ARCH_DIR"/}" = "$ROADMAP_REL_PATH" ] && spec_exists "$PENDING_DIR" "$base"; then
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
      err "test_arch_M_references_resolve: $ref cited in $src resolves to no spec in $DONE_DIR/ or $ACTIVE_DIR/ (pending/ resolves only in $ROADMAP_REL_PATH)"
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
  stale_schedule_hits=$(grep -rEn "NullClaw-managed schedule|cron_add.*schedule" "$ARCH_DIR" 2>/dev/null || true)
  if [ -n "$stale_schedule_hits" ]; then
    err "architecture_schedule_ownership: stale local-scheduler ownership text found:"
    printf "%s\n" "$stale_schedule_hits" >&2
  fi
  [ "$FAIL" = 0 ] && ok "architecture_schedule_ownership: QStash/agentsfleetd ownership is consistent"
fi

# ---------------------------------------------------------------------------
# Citation assertions. The four checks above ask whether the docs
# point at real specs and real pages. These four ask whether they describe a
# real tree: a page naming a dropped table or a renumbered slot reads as current
# and is worse than a page that says nothing.
#
# The set is wider than ARCH_DIR — the auth and development pages carry the same
# citations and drift the same way — but only when ARCH_DIR is the real corpus,
# so a fixture run never reaches out and grades the live pages by accident.
# ---------------------------------------------------------------------------

readonly DEFAULT_ARCH_DIR="docs/architecture"
if [ "$ARCH_DIR" = "$DEFAULT_ARCH_DIR" ]; then
  DOC_SET_EXTRA="${DOC_SET_EXTRA:-docs/AUTH.md docs/AUTH_DEVICE_LOGIN.md docs/development.md}"
else
  DOC_SET_EXTRA="${DOC_SET_EXTRA:-}"
fi

# Qualified names that look like `schema.table` but are not tables. Each entry
# needs a reason; an unexplained entry is a table the check stopped guarding.
#   fleet.delivery — an OpenTelemetry span name (semconv.SPAN_FLEET_DELIVERY).
readonly NON_TABLE_QUALIFIED_NAMES="fleet.delivery"

# Tables a page may name because it is recording that they are gone. Naming one
# is a deliberate retirement note, not a claim that it is live storage. Adding an
# entry here is a decision; leaving one behind after the note goes is drift.
#   fleet.metering_periods — dropped in the schema rebuild; the billing page and
#   the roadmap both explain what replaced it.
#   core.fleet_bundles — the per-workspace bundle table; the fleet-bundles page
#   records that install resolves from a library tier instead.
readonly RETIRED_TABLES="fleet.metering_periods core.fleet_bundles"

# Files that belong to a sibling project rather than this repository. A page may
# name one because the behaviour it describes lives there.
#   NullClaw's provider routing — the fleet loop is its own codebase.
readonly EXTERNAL_PROJECT_PATHS="compatible.zig providers/factory.zig nullclaw/src/providers/factory.zig"

doc_files() {
  find "$ARCH_DIR" -name '*.md' 2>/dev/null | sort
  local extra
  for extra in $DOC_SET_EXTRA; do
    [ -f "$extra" ] && printf '%s\n' "$extra"
  done
}

# ---------------------------------------------------------------------------
# 5. test_arch_cited_paths_resolve
#    Pages cite files two ways: in full from the repository root, and in a
#    readable shorthand that drops the leading directories (`http/router.zig`).
#    Both are fine. A path that matches no tracked file either way is not.
# ---------------------------------------------------------------------------
TRACKED_FILES="$(git ls-files 2>/dev/null || true)"
cited_paths=0
broken_paths=0
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  src="${entry%%::*}"
  path="${entry##*::}"
  # A `~/`-anchored path says outside this repository on its face; the operating
  # model checkout is cited that way throughout. Nothing to resolve here.
  case "$path" in "~/"*) continue;; esac
  case " $EXTERNAL_PROJECT_PATHS " in *" $path "*) continue;; esac
  cited_paths=$((cited_paths + 1))
  # Here-strings, not pipes: `grep -q` closes the pipe on its first match, and
  # under `pipefail` that SIGPIPE becomes the pipeline's status, so every match
  # would read as a miss.
  grep -qx -- "$path" <<<"$TRACKED_FILES" && continue
  grep -q -- "/$path\$" <<<"$TRACKED_FILES" && continue
  err "test_arch_cited_paths_resolve: $src cites '$path', which matches no tracked file"
  broken_paths=$((broken_paths + 1))
done < <(doc_files | while IFS= read -r f; do
  grep -oE '`[A-Za-z0-9_][A-Za-z0-9_./-]*\.(zig|sql|ts|tsx|py|sh|mk)`' "$f" 2>/dev/null \
    | tr -d '`' | sort -u | sed "s|^|$f::|" || true
done)
[ "$broken_paths" = 0 ] && ok "test_arch_cited_paths_resolve: all $cited_paths cited source paths resolve"

# ---------------------------------------------------------------------------
# 6. test_arch_cited_tables_exist
#    A schema-qualified name in a page is a claim that the table is live
#    storage. schema/ is the authority.
# ---------------------------------------------------------------------------
broken_tables=0
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  src="${entry%%::*}"
  name="${entry##*::}"
  case " $NON_TABLE_QUALIFIED_NAMES " in *" $name "*) continue;; esac
  case " $RETIRED_TABLES " in *" $name "*) continue;; esac
  # `memory.md` reads as schema.table under the same pattern. A filename is not
  # a claim about storage, so drop anything whose tail is a file extension.
  case "$name" in *.md|*.zig|*.sql|*.ts|*.py|*.sh|*.json|*.yaml) continue;; esac
  grep -rqE "CREATE TABLE IF NOT EXISTS[[:space:]]+$name\b" schema/ 2>/dev/null && continue
  err "test_arch_cited_tables_exist: $src names '$name', which schema/ does not define"
  broken_tables=$((broken_tables + 1))
done < <(doc_files | while IFS= read -r f; do
  # Backtick-anchored, so `agentsfleet.billing.charge.type` (a metric name) is
  # not read as a table. A trailing `.column` is kept in scope because pages name
  # a column to make the claim concrete: `core.tenant_billing.balance_nanos`.
  grep -oE '`(core|billing|fleet|memory|vault)\.[a-z_]+(\.[a-z_]+)?[` ]' "$f" 2>/dev/null \
    | sed -E 's/^`//; s/[` ]$//; s/^([a-z]+\.[a-z_]+)\..*/\1/' \
    | sort -u | sed "s|^|$f::|" || true
done)
[ "$broken_tables" = 0 ] && ok "test_arch_cited_tables_exist: every named table exists in schema/"

# ---------------------------------------------------------------------------
# 7. test_arch_cited_make_targets_exist
#    A page telling a contributor to run a target that no makefile declares
#    costs them the time it takes to discover that.
# ---------------------------------------------------------------------------
broken_targets=0
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  src="${entry%%::*}"
  target="${entry##*::}"
  grep -rqE "^$target:" Makefile make/ 2>/dev/null && continue
  err "test_arch_cited_make_targets_exist: $src names 'make $target', which no makefile declares"
  broken_targets=$((broken_targets + 1))
done < <(doc_files | while IFS= read -r f; do
  grep -oE '`make [a-z][a-z0-9_.-]*`' "$f" 2>/dev/null \
    | sed 's|`make ||;s|`||' | sort -u | sed "s|^|$f::|" || true
done)
[ "$broken_targets" = 0 ] && ok "test_arch_cited_make_targets_exist: every named make target exists"

# ---------------------------------------------------------------------------
# 8. test_arch_section_anchors_resolve
#    The file check above proves the page exists; this proves the section does.
#    `check_architecture_doc_anchors.sh` owns the pointer spellings — the
#    section may sit after the link or inside its text, quoted or bare — and
#    hands back `src::target::anchor` triples for this loop to resolve.
# ---------------------------------------------------------------------------
broken_anchors=0
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  src="${entry%%::*}"
  rest="${entry#*::}"
  target="${rest%%::*}"
  anchor="${rest##*::}"
  target_path="$(dirname "$src")/$target"
  [ -f "$target_path" ] || continue
  # Prefix-anchored, not substring. A pointer names the heading's opening words,
  # so `^#+ <anchor>` is the true test. Substring matching let a short anchor —
  # `§C. EXECUTE` truncated to `C` — match any heading on the page and report
  # green, which is the silent pass this assertion exists to prevent.
  grep -qiE "^#+[[:space:]]+$(printf '%s' "$anchor" | sed 's/[][(){}|+?\\.*^$/]/\\&/g')" "$target_path" && continue
  err "test_arch_section_anchors_resolve: $src points at $target §$anchor, which is not a heading there"
  broken_anchors=$((broken_anchors + 1))
done < <(doc_files | bash "$SCRIPT_DIR/check_architecture_doc_anchors.sh")
[ "$broken_anchors" = 0 ] && ok "test_arch_section_anchors_resolve: every cross-page section anchor resolves"

# ---------------------------------------------------------------------------
# 9. test_arch_no_retired_slot_numbers
#    Slot numbering starts at 1xx: `schema/embed.zig` records that 001–046 are
#    retired wholesale, so no new slot can reuse one. A page citing a `0xx` slot
#    is therefore describing a schema that no longer exists, whether it writes
#    the number as a filename or as prose.
# ---------------------------------------------------------------------------
# Link text is stripped first: a published decision record keeps the title it was
# published under, and renaming it in a citation would point at the wrong thing.
retired_slot_hits=""
while IFS= read -r f; do
  hits="$(sed -E 's/\[[^]]*\]\([^)]*\)//g' "$f" \
    | grep -nEo '([Ss]lots?[- ]`?|schema/)0[0-9][0-9]' 2>/dev/null || true)"
  [ -n "$hits" ] && retired_slot_hits="$retired_slot_hits$f:$hits"$'\n'
done < <(doc_files)
if [ -n "${retired_slot_hits// /}" ]; then
  err "test_arch_no_retired_slot_numbers: pages cite schema slots retired by the renumbering:"
  printf "%s" "$retired_slot_hits" >&2
else
  ok "test_arch_no_retired_slot_numbers: no page cites a retired 0xx schema slot"
fi

# A pattern that silently matches nothing reports clean forever. Against the real
# corpus the citation count is in the hundreds; a collapse to zero means the
# extraction broke, not that the pages stopped citing anything.
readonly CITATION_FLOOR=25
if [ "$ARCH_DIR" = "$DEFAULT_ARCH_DIR" ] && [ "$cited_paths" -lt "$CITATION_FLOOR" ]; then
  err "citation extraction found only $cited_paths source paths in the real corpus — the pattern is broken, not the docs"
fi

# ---------------------------------------------------------------------------
exit "$FAIL"
