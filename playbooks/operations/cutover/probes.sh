#!/usr/bin/env bash
# The cutover probe runner, and the assert that keeps it honest.
#
#     bash playbooks/operations/cutover/probes.sh              # run the probes
#     bash playbooks/operations/cutover/probes.sh --coverage   # coverage assert only
#     bash playbooks/operations/cutover/probes.sh --self-test  # the runner's own tests
#
# Tests covered:
#   * test_probe_runner_row_coverage — an uncovered row, an untagged probe and
#     an undeclared skip each fail; a complete set passes
#   * test_rollback_carries_no_migrate — the runbook's rollback section invokes
#     no migration command, asserted rather than read
#   * test_architecture_matches_deployed_metrics_path — no architecture document
#     claims a scrape configuration the deployment does not have
#
# THE ASSERT IS OVER ROWS, NOT PROBES. Counting probes measures how much was
# written; counting rows measures how much is covered. A runner with forty
# probes and an unprobed rubric row is the shape this refuses: every row of the
# merged milestones is either tagged by at least one probe, or named in the
# exclusion manifest with a reason — and the manifest is printed on every run,
# so a skip cannot become invisible by being old.
#
# THE ROW SET IS DERIVED, NOT LISTED. Rows are read out of the merged
# milestones' own rubric tables, so a milestone that adds a row adds it here
# too. A hand-kept list is the one somebody forgets to update, and the
# forgotten row is exactly what this exists to catch.
#
# Exits 0 on success, 1 on the first failing assertion (with diagnostic).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
readonly SCRIPT_DIR REPO_ROOT

# Overridable so the self-tests can point the runner at fixtures. Nothing else
# sets them.
SPEC_DONE_DIR="${SPEC_DONE_DIR:-$REPO_ROOT/docs/v2/done}"
RUNBOOK="${RUNBOOK:-$SCRIPT_DIR/001_playbook.md}"
ARCH_DIR="${ARCH_DIR:-$REPO_ROOT/docs/architecture}"
COVERAGE_TSV="${COVERAGE_TSV:-$SCRIPT_DIR/coverage.tsv}"
DEPLOY_DIR="${DEPLOY_DIR:-$REPO_ROOT/deploy}"
readonly SPEC_DONE_DIR RUNBOOK ARCH_DIR DEPLOY_DIR COVERAGE_TSV

# Everything milestone-shaped lives in `coverage.tsv` — the merged milestone
# list, which probe covers which rows, and which rows are deliberately skipped.
# Read from data rather than written here because a rubric row identifier IS a
# milestone identifier, and RULE TST-NAM bars those from source.
tsv() { grep -v '^#' "$COVERAGE_TSV" | awk -F'\t' -v k="$1" 'NF > 1 && $1 == k'; }

merged_milestones() { tsv milestone | cut -f2; }

# `*` in a row list means "this row, in every merged milestone" — the
# repository-hygiene rows are the same claim five times and writing them out
# five times is five places to forget one.
expand_rows() {
  local spec milestone
  for spec in $1; do
    case "$spec" in
      \*:*) for milestone in $(merged_milestones); do printf '%s:%s\n' "$milestone" "${spec#*:}"; done ;;
      *)    printf '%s\n' "$spec" ;;
    esac
  done
}

FAIL=0
err() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()  { printf "OK:   %s\n" "$*"; }

# ---------------------------------------------------------------------------
# The row set, read from the merged milestones' own rubric tables.
# ---------------------------------------------------------------------------
all_rows() {
  local milestone spec
  for milestone in $(merged_milestones); do
    spec="$(find "$SPEC_DONE_DIR" -maxdepth 1 -name "${milestone}*.md" 2>/dev/null | head -1)"
    [ -n "$spec" ] || { err "no merged spec found for $milestone under $SPEC_DONE_DIR"; continue; }
    sed -n 's/^| *\([RS][0-9][0-9]*\) *|.*/\1/p' "$spec" \
      | sed "s|^|${milestone}:|"
  done | sort -u
}

# ---------------------------------------------------------------------------
# The probes. Each declares the rows it covers; `probe_rows` is the tag the
# coverage assert reads, so a probe that grades nothing declares nothing and
# is caught rather than counted.
# ---------------------------------------------------------------------------

# A probe is `probe <name> <rows...> -- <command...>`. The command runs only in
# a full run; --coverage reads the tags without executing anything, which is
# what lets the assert run in `lint-all` with no environment.
PROBE_NAMES=()
PROBE_TAGS=()
PROBE_CMDS=()

# A probe names itself and its command; the rows it covers are looked up in
# `coverage.tsv` by that name. A probe the table does not mention covers
# nothing, and `assert_row_coverage` says so rather than counting it.
probe() {
  local name="$1"; shift
  [ "${1:-}" = "--" ] && shift
  PROBE_NAMES+=("$name")
  PROBE_TAGS+=("$(expand_rows "$(tsv covers | awk -F'	' -v n="$name" '$2 == n {print $3}')" | tr '
' ' ')")
  PROBE_CMDS+=("$*")
}

probe conform  -- make harness-verify
probe unit     -- make test-unit-all
probe lint     -- make lint-all
probe version  -- make check-version
probe secrets  -- gitleaks detect --no-banner
probe boot_ready       -- 'curl -fsS "${BASE_URL:?BASE_URL required for a live probe}/readyz"'
probe route_contract   -- 'make test-parity BASE_URL="${BASE_URL:?BASE_URL required for a live probe}"'
probe substrate_suite  -- make test-integration-rustd
probe runner_plane     -- make test-integration-rustd
probe tenant_surface   -- make test-integration-rustd
probe operator_surface -- make test-integration-rustd
probe scaffold         -- make test-unit-all

# ---------------------------------------------------------------------------
# The exclusion manifest. Printed on EVERY run — a skip that becomes invisible
# by being old is the failure this format prevents. One row per line:
#   <milestone>:<row>|<reason>
# ---------------------------------------------------------------------------

exclusion_rows() { expand_rows "$(tsv exclude | cut -f2 | tr '\n' ' ')" | sort -u; }

print_manifest() {
  printf '\n  Exclusion manifest — rows deliberately not probed:\n'
  tsv exclude | while IFS=$'\t' read -r _ row reason; do
    [ -n "$row" ] && printf '    %-16s %s\n' "$row" "$reason"
  done
  printf '\n'
}

# ---------------------------------------------------------------------------
# test_probe_runner_row_coverage — three ways this fails, all of them real.
# ---------------------------------------------------------------------------
tagged_rows() { printf '%s\n' "${PROBE_TAGS[@]}" | tr ' ' '\n' | grep . | sort -u; }

assert_row_coverage() {
  local rows tagged excluded covered uncovered phantom row reason
  rows="$(all_rows)"
  tagged="$(tagged_rows)"
  excluded="$(exclusion_rows)"
  covered="$(printf '%s\n%s\n' "$tagged" "$excluded" | grep . | sort -u)"

  # Set arithmetic rather than a grep per row: three `comm` passes replace ~170
  # subprocesses and take the assert from two seconds to well under one, which
  # is what lets the self-tests — nine runs of this — ride `lint-all`. Every
  # input is already `sort -u`'d, which is `comm`'s precondition.

  # 1. Every row is probed or declared. An unprobed, undeclared row is a claim
  #    the swap would inherit with nothing re-proving it.
  uncovered="$(comm -23 <(printf '%s\n' "$rows" | grep .) <(printf '%s\n' "$covered"))"
  while read -r row; do
    [ -n "$row" ] && err "rubric row $row is neither probed nor in the exclusion manifest"
  done <<<"$uncovered"

  # 2. Every probe tag names a row that exists. A tag pointing at a renamed or
  #    deleted row is coverage on paper only.
  phantom="$(comm -13 <(printf '%s\n' "$rows" | grep .) <(printf '%s\n' "$tagged" | grep .))"
  while read -r row; do
    [ -n "$row" ] && err "a probe tags $row, which is not a rubric row of any merged milestone"
  done <<<"$phantom"

  # 3. Every exclusion names a row that exists, and carries a reason. A skip
  #    without a reason is the thing the manifest exists to prevent.
  # An exclusion may name a row pattern (`*:S6`), so each is expanded before it
  # is checked — and every expansion of it must name a real row, or the skip
  # covers nothing.
  local expanded
  while IFS=$'\t' read -r _ spec reason; do
    [ -n "$spec" ] || continue
    [ -n "${reason// /}" ] || err "exclusion $spec carries no reason"
    while read -r expanded; do
      [ -n "$expanded" ] || continue
      printf '%s\n' "$rows" | grep -qxF "$expanded" \
        || err "exclusion names $expanded, which is not a rubric row"
    done < <(expand_rows "$spec")
  done < <(tsv exclude)

  # A probe declaring no rows grades nothing it can be held to.
  local i
  for i in "${!PROBE_NAMES[@]}"; do
    [ -n "${PROBE_TAGS[$i]}" ] || err "probe '${PROBE_NAMES[$i]}' declares no rubric row"
  done

  [ "$FAIL" -eq 0 ] && ok "$(printf '%s\n' "$rows" | grep -c .) rubric rows: all probed or declared"
}

# ---------------------------------------------------------------------------
# test_rollback_carries_no_migrate — asserted, not read.
# ---------------------------------------------------------------------------
# A migration in a rollback path is at best a no-op and at worst the one command
# that can refuse mid-incident. The runbook says so in prose; this is what makes
# the prose true.
readonly MIGRATE_PATTERN='(^|[^a-z_-])(migrate|migration|sqlx +migrate|db:migrate)([^a-z_-]|$)'

assert_rollback_has_no_migrate() {
  [ -f "$RUNBOOK" ] || { err "no runbook at $RUNBOOK"; return; }
  local section
  section="$(sed -n '/^## The one-move rollback/,/^## /p' "$RUNBOOK")"
  [ -n "$section" ] || { err "$RUNBOOK has no '## The one-move rollback' section to assert over"; return; }
  # Backticked commands only: the section is ALLOWED to say the word "migration"
  # while explaining why it invokes none, and a check that cannot tell prose
  # from a command would forbid the explanation.
  local offenders
  offenders="$(printf '%s\n' "$section" | grep -oE '`[^`]+`' | grep -iE "$MIGRATE_PATTERN" || true)"
  if [ -n "$offenders" ]; then
    err "the rollback section invokes a migration command: $offenders"
  else
    ok "the rollback path invokes no migration command"
  fi
}

# ---------------------------------------------------------------------------
# test_architecture_matches_deployed_metrics_path
# ---------------------------------------------------------------------------
# A milestone that grades metric continuity cannot cite a document describing a
# scrape path the deployment does not have. The rule is one-directional: a
# document may say the daemon is NOT scraped whatever the configuration holds,
# but it may not claim a scrape the configuration does not declare.
readonly SCRAPE_CLAIM_PATTERN='\[\[metrics\]\]|scrapes .*(/metrics|:9091)|Prometheus scrapes'
readonly DEPLOY_METRICS_PATTERN='\[\[metrics\]\]'

assert_architecture_matches_deployment() {
  [ -d "$ARCH_DIR" ] || { err "no architecture directory at $ARCH_DIR"; return; }
  local deployed_blocks claims
  deployed_blocks="$(grep -rlE "$DEPLOY_METRICS_PATTERN" "$DEPLOY_DIR" 2>/dev/null || true)"
  claims="$(grep -rnE "$SCRAPE_CLAIM_PATTERN" "$ARCH_DIR" --include='*.md' 2>/dev/null || true)"

  if [ -z "$claims" ]; then
    ok "no architecture document claims a scrape path"
    return
  fi
  if [ -n "$deployed_blocks" ]; then
    ok "architecture claims a scrape path and the deployment declares one"
    return
  fi
  err "architecture claims a scrape path the deployment does not declare:"
  printf '%s\n' "$claims" | sed 's|^|      |' >&2
  printf '      no [[metrics]] block under %s\n' "$DEPLOY_DIR" >&2
}

# ---------------------------------------------------------------------------
run_probes() {
  local i name cmd
  for i in "${!PROBE_NAMES[@]}"; do
    name="${PROBE_NAMES[$i]}"; cmd="${PROBE_CMDS[$i]}"
    printf '→ probe %s (%s)\n' "$name" "${PROBE_TAGS[$i]}"
    # Deliberately not `eval` on anything a caller supplies: every command here
    # is a literal in this file. A probe that reads BASE_URL quotes it SINGLY at
    # declaration so the expansion happens when the probe runs — expanding at
    # declaration made `--coverage`, which executes nothing, demand a live URL.
    if bash -c "$cmd"; then ok "probe $name"; else err "probe $name failed"; fi
  done
}

usage() {
  sed -n '2,8p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

main() {
  case "${1:-}" in
    --self-test) exec bash "$SCRIPT_DIR/probes_test.sh" ;;
    --help|-h)   usage; exit 0 ;;
  esac

  cd "$REPO_ROOT"
  assert_row_coverage
  assert_rollback_has_no_migrate
  assert_architecture_matches_deployment
  print_manifest

  if [ "${1:-}" != "--coverage" ]; then
    [ "$FAIL" -eq 0 ] || { printf 'refusing to run probes while an assert is red\n' >&2; exit 1; }
    run_probes
  fi
  exit "$FAIL"
}

main "$@"
