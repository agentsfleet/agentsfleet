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
#   fleet.metering_periods — dropped in the schema rebuild; the billing page
#   explains what replaced it.
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

# Every tracked Markdown file a reader can follow a published link from, which is
# wider than `doc_files` on purpose — see test_arch_published_links_resolve.
# Falls back to `doc_files` when the tree is not a checkout, which is how the
# fixture-driven self-tests run.
published_link_files() {
  if [ "$ARCH_DIR" = "$DEFAULT_ARCH_DIR" ] && git rev-parse --git-dir >/dev/null 2>&1; then
    git ls-files '*.md' | grep -v '^docs/v2/' | sort
  else
    doc_files
  fi
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
  grep -oE '`[A-Za-z0-9_][A-Za-z0-9_./-]*\.(zig|rs|sql|ts|tsx|py|sh|mk)`' "$f" 2>/dev/null \
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
#    `check_architecture_doc_anchors.sh` owns the pointer spellings and hands
#    back `src::target::anchor` triples for this loop to resolve.
# ---------------------------------------------------------------------------
broken_anchors=0
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  src="${entry%%::*}"
  rest="${entry#*::}"
  target="${rest%%::*}"
  anchor="${rest##*::}"
  # `@self` — no link ahead of it, so it must name a heading on its own page.
  if [ "$target" = "@self" ]; then target_path="$src"; else target_path="$(dirname "$src")/$target"; fi
  [ -f "$target_path" ] || continue
  # Exactly one match, prefix-anchored. A pointer names a heading's opening
  # words, so `^#+ <anchor>` is the test. Several matches prove nothing: `§C`
  # prefix-matches `## Config`, `## Connection topology` and `## Concrete
  # example` alike, naming none. Same fix — quote the full heading text.
  hits="$(grep -ciE "^#+[[:space:]]+$(printf '%s' "$anchor" | sed 's/[][(){}|+?\\.*^$/]/\\&/g')" "$target_path" || true)"
  [ "$hits" = 1 ] && continue
  [ "$target" = "@self" ] && where="its own page" || where="$target"
  [ "$hits" = 0 ] \
    && err "test_arch_section_anchors_resolve: $src §$anchor names no heading on $where — quote the anchor and move any descriptor outside it, or add a link if the section lives on another page" \
    || err "test_arch_section_anchors_resolve: $src §$anchor prefix-matches $hits headings on $where — quote the full heading text"
  broken_anchors=$((broken_anchors + 1))
done < <(doc_files | bash "$SCRIPT_DIR/check_architecture_doc_anchors.sh")
[ "$broken_anchors" = 0 ] && ok "test_arch_section_anchors_resolve: every cross-page section anchor resolves"

# ---------------------------------------------------------------------------
# 9. test_arch_no_retired_slot_numbers
#    Slot numbering starts at 1xx: the rebuild retired 001–046 wholesale and no
#    new slot reuses one, which is why the ledger in afd_db/src/migration.rs
#    opens at 100. A page citing a `0xx` slot describes a schema that no longer
#    exists, whether it writes the number as a filename or as prose.
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

# ---------------------------------------------------------------------------
# 10. test_the_doc_carries_no_conflict_marker
#     A shipped spec recorded this fault class and named this test by name; the
#     test was never in the tree, and seven markers were sitting in five pages
#     when the docs review of 2026-09-17 found them. One reaches the branch by
#     riding the END of a sentence or a table row, so a line-anchored `^>>>>>>>`
#     — which is what every pre-commit grep uses — looks straight past it. Match
#     anywhere in the line. `=======` is excluded on purpose: it is a legal
#     Markdown setext rule and would fire on real prose.
# ---------------------------------------------------------------------------
marker_hits=""
while IFS= read -r f; do
  hits="$(grep -nE '(<{7}|>{7}) ' "$f" 2>/dev/null || true)"
  [ -n "$hits" ] && marker_hits="$marker_hits$f:$hits"$'\n'
done < <(doc_files)
if [ -n "${marker_hits// /}" ]; then
  err "test_the_doc_carries_no_conflict_marker: merge conflict markers survived a merge:"
  printf "%s" "$marker_hits" >&2
else
  ok "test_the_doc_carries_no_conflict_marker: no page carries a merge conflict marker"
fi

# ---------------------------------------------------------------------------
# 11. test_arch_published_links_resolve
#     Every docs.agentsfleet.net pointer must name a page that exists in the
#     published set. A pointer to a page nobody wrote sends the reader to a 404,
#     and the docs repository is a sibling checkout the gate cannot assume, so
#     the roster below is the contract.
#
#     This one check reads EVERY tracked Markdown file, not `doc_files`. The
#     other checks are scoped to the architecture set because that is the corpus
#     they grade; a dead published link is a dead link wherever it sits, and the
#     first review of this gate found one in `SKILL_FRONTMATTER_SCHEMA.md` —
#     outside `doc_files`, so the check as first written would have passed it.
#     Specs under `docs/v2/` are excluded: they are records, and DOC-S7 keeps a
#     record's wording even when the page it cited has moved.
#
#     Regenerate the roster with:
#       find ~/Projects/docs -name '*.mdx' -not -path '*/snippets/*' \
#         | sed 's|.*/docs/||; s|\.mdx$||' | sort
#     A page added there and not added here fails closed — the right failure,
#     since a stale roster is invisible and a stale pointer is a dead link.
# ---------------------------------------------------------------------------
readonly PUBLISHED_PAGES="
api-reference/error-codes api-reference/introduction api-reference/scopes
billing/budgets changelog cli/agentsfleet cli/configuration cli/flags cli/install
concepts concepts/context-lifecycle fleets/authoring fleets/connectors
fleets/install fleets/library fleets/model-providers fleets/overview
fleets/running fleets/secrets fleets/tools fleets/troubleshooting fleets/webhooks
index memory quickstart runners workspaces/managing workspaces/overview
"
# Word-split and rejoin on single spaces: the roster above is newline-wrapped for
# reading, and a `case` glob testing " $slug " never matches at a line boundary.
published_flat=" $(printf '%s ' $PUBLISHED_PAGES) "
bad_published=0
seen_published=0
while IFS= read -r ref; do
  src="${ref%%::*}"
  slug="${ref##*::}"
  seen_published=$((seen_published + 1))
  case "$published_flat" in
    *" $slug "*) continue ;;
  esac
  err "test_arch_published_links_resolve: $src points at docs.agentsfleet.net/$slug, which is not a published page"
  bad_published=$((bad_published + 1))
done < <(
  # The filename is carried by the shell, never interpolated into a `sed`
  # replacement: a path holding `&` or `|` would otherwise rewrite the match or
  # break the expression, and the failure would name the wrong file.
  while IFS= read -r f; do
    # Fenced blocks are skipped: a URL inside one is example output or an
    # identifier, not a link a reader clicks. The CLI's rendered `see:` line and
    # an RFC 7807 `type` member both spell a docs URL and neither is navigation,
    # so failing on them would force an example to be written wrong to stay green.
    awk '/^[[:space:]]*```/ { fence = !fence; next } !fence' "$f" 2>/dev/null \
      | grep -oE 'docs\.agentsfleet\.net/[A-Za-z0-9/_-]+' \
      | while IFS= read -r hit; do
          printf '%s::%s\n' "$f" "${hit#docs.agentsfleet.net/}"
        done || true
  done < <(published_link_files) | sort -u
)

# The same guard `CITATION_FLOOR` gives the citation pattern, for the same
# reason: a check that extracts nothing reports green forever, so a broken
# regex would read as "every pointer resolves" rather than "no pointer was
# read". The real corpus carried 17 when this landed.
readonly PUBLISHED_REF_FLOOR=10
if [ "$ARCH_DIR" = "$DEFAULT_ARCH_DIR" ] && [ "$seen_published" -lt "$PUBLISHED_REF_FLOOR" ]; then
  err "test_arch_published_links_resolve: extraction found only $seen_published pointers in the real corpus — the pattern is broken, not the docs"
elif [ "$bad_published" = 0 ]; then
  ok "test_arch_published_links_resolve: all $seen_published docs.agentsfleet.net pointers name a published page"
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
