#!/usr/bin/env bash
# Self-tests for probes.sh — the coverage assert, the rollback assert, and the
# architecture assert.
#
#     bash playbooks/operations/cutover/probes_test.sh
#     bash playbooks/operations/cutover/probes.sh --self-test
#
# The runner drives fixtures through SPEC_DONE_DIR, RUNBOOK, ARCH_DIR and
# DEPLOY_DIR, so every test here exercises the SAME assert code a real run does.
# All of them use `--coverage`, which reads probe tags without executing a
# probe — that is what lets the suite ride `make lint-all` with no environment.
#
# Covers Dimension 5.1's `test_probe_runner_row_coverage`, plus the negative
# halves of 5.2 and 5.3.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly RUNNER="$SCRIPT_DIR/probes.sh"
readonly COVERAGE="$SCRIPT_DIR/coverage.tsv"

# The milestone list is READ, never written here: a literal milestone identifier
# in source is what RULE TST-NAM bars, and a fixture set that hardcodes one
# stops tracking the table the runner actually grades against.
milestones() { grep -v '^#' "$COVERAGE" | awk -F'\t' '$1 == "milestone" {print $2}'; }
nth_milestone() { milestones | sed -n "${1}p"; }

# The rows the probes tag, plus the rows the manifest declares. A fixture spec
# set carrying exactly these is the "complete set" case; anything added to it is
# an uncovered row. Derived from the same table the runner reads, so a probe
# added there does not silently stop being covered here.
fixture_rows_for() {
  local milestone="$1"
  {
    grep -v '^#' "$COVERAGE" | awk -F'	' '$1 == "covers" {print $3}'
    grep -v '^#' "$COVERAGE" | awk -F'	' '$1 == "exclude" {print $2}'
  } | tr ' ' '
' | grep . \
    | sed "s|^\*:|${milestone}:|" \
    | grep "^${milestone}:" | cut -d: -f2 | sort -u | tr '
' ' '
}

passed=0
failed=0
ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# A spec carrying a rubric table with the given rows, in the shape the row
# reader parses out of a real merged spec.
write_spec() {
  local dir="$1" milestone="$2" rows="$3" row
  mkdir -p "$dir"
  {
    printf '# %s fixture\n\n## Acceptance Rubric (single scoring surface)\n\n' "$milestone"
    printf '| # | Criterion | Verify | Expected | Priority | Graded |\n'
    printf '|---|---|---|---|---|---|\n'
    for row in $rows; do
      printf '| %s | fixture criterion | `true` | exit 0 | P0 | |\n' "$row"
    done
  } >"$dir/${milestone}_P0_FIXTURE.md"
}

# A complete fixture spec set: exactly the rows the probes and manifest cover.
build_specs() {
  local dir="$1" extra="${2:-}" milestone rows i=0
  rm -rf "$dir"; mkdir -p "$dir"
  while read -r milestone; do
    [ -n "$milestone" ] || continue
    i=$((i + 1))
    rows="$(fixture_rows_for "$milestone")"
    # The extra row lands on the SECOND milestone only, so an uncovered-row test
    # names one row rather than five.
    [ "$i" = "2" ] && rows="$rows${extra:+ $extra}"
    write_spec "$dir" "$milestone" "$rows"
  done < <(milestones)
}

write_runbook() {
  local path="$1" rollback_body="$2"
  mkdir -p "$(dirname "$path")"
  printf '# fixture runbook\n\n## The one-move rollback\n\n%s\n\n## Evidence\n\nnone.\n' \
    "$rollback_body" >"$path"
}

write_arch() {
  local dir="$1" body="$2"
  rm -rf "$dir"; mkdir -p "$dir"
  printf '# fixture architecture\n\n%s\n' "$body" >"$dir/observability.md"
}

write_deploy() {
  local dir="$1" with_block="$2"
  rm -rf "$dir"; mkdir -p "$dir/fly/agentsfleetd-prod"
  if [ "$with_block" = "yes" ]; then
    printf '[[metrics]]\n  port = 9091\n  path = "/metrics"\n' >"$dir/fly/agentsfleetd-prod/fly.toml"
  else
    printf 'app = "agentsfleetd-prod"\n' >"$dir/fly/agentsfleetd-prod/fly.toml"
  fi
}

run_runner() {
  env SPEC_DONE_DIR="$1" RUNBOOK="$2" ARCH_DIR="$3" DEPLOY_DIR="$4" \
    bash "$RUNNER" --coverage >"$WORK_DIR/out" 2>&1
  printf '%s' "$?"
}
out() { cat "$WORK_DIR/out"; }

SPECS="$WORK_DIR/specs"
RUNBOOK_OK="$WORK_DIR/rb_ok.md"
ARCH="$WORK_DIR/arch"
DEPLOY="$WORK_DIR/deploy"

write_runbook "$RUNBOOK_OK" 'Drain, serve the previous image digest, wait for `/readyz`, return to the balancer. No migration is invoked here.'
write_arch "$ARCH" 'The daemon pushes OTLP to one configured endpoint. Nothing scrapes it.'
write_deploy "$DEPLOY" no

# --------------------------------------------------------------------------
# The baseline. Every negative below is only meaningful if this is green — a
# runner that failed on everything would "detect" every seeded fault while
# proving nothing.
# --------------------------------------------------------------------------
build_specs "$SPECS"
status="$(run_runner "$SPECS" "$RUNBOOK_OK" "$ARCH" "$DEPLOY")"
if [ "$status" = "0" ]; then
  ok "a complete set passes"
else
  bad "a complete set passes" "exit $status: $(out)"
fi

# --------------------------------------------------------------------------
# test_probe_runner_row_coverage — an uncovered row.
# --------------------------------------------------------------------------
build_specs "$SPECS" "R9"
status="$(run_runner "$SPECS" "$RUNBOOK_OK" "$ARCH" "$DEPLOY")"
if [ "$status" != "0" ] && out | grep -qF "$(nth_milestone 2):R9"; then
  ok "an uncovered rubric row fails, naming the row"
else
  bad "an uncovered rubric row fails" "exit $status: $(out)"
fi

# A probe tagging a row that no longer exists is coverage on paper only.
build_specs "$SPECS"
rm -f "$SPECS/$(nth_milestone 5)_P0_FIXTURE.md"
write_spec "$SPECS" "$(nth_milestone 5)" "R1 S1 S2 S3 S4 S5 S6"
status="$(run_runner "$SPECS" "$RUNBOOK_OK" "$ARCH" "$DEPLOY")"
if [ "$status" != "0" ] && out | grep -qF "which is not a rubric row"; then
  ok "a probe tagging a row that does not exist fails"
else
  bad "a probe tagging a row that does not exist fails" "exit $status: $(out)"
fi

# An exclusion naming a row that does not exist is a skip of nothing.
build_specs "$SPECS"
rm -f "$SPECS/$(nth_milestone 1)_P0_FIXTURE.md"
write_spec "$SPECS" "$(nth_milestone 1)" "R1 R2 R3 R4 R5 S1 S2 S3 S4 S5"
status="$(run_runner "$SPECS" "$RUNBOOK_OK" "$ARCH" "$DEPLOY")"
if [ "$status" != "0" ] && out | grep -qE "exclusion names|not a rubric row"; then
  ok "an exclusion naming a row that does not exist fails"
else
  bad "an exclusion naming a row that does not exist fails" "exit $status: $(out)"
fi

# --------------------------------------------------------------------------
# test_rollback_carries_no_migrate
# --------------------------------------------------------------------------
build_specs "$SPECS"
RUNBOOK_BAD="$WORK_DIR/rb_migrate.md"
write_runbook "$RUNBOOK_BAD" 'Drain, then run `agentsfleetd migrate` before serving the previous digest.'
status="$(run_runner "$SPECS" "$RUNBOOK_BAD" "$ARCH" "$DEPLOY")"
if [ "$status" != "0" ] && out | grep -qF "invokes a migration command"; then
  ok "a rollback section invoking a migration fails"
else
  bad "a rollback section invoking a migration fails" "exit $status: $(out)"
fi

# The section is allowed to SAY "migration" while explaining it runs none — a
# check that cannot tell prose from a command would forbid the explanation.
RUNBOOK_PROSE="$WORK_DIR/rb_prose.md"
write_runbook "$RUNBOOK_PROSE" 'This path invokes no migration command, because a migration mid-incident is the one command that can refuse.'
status="$(run_runner "$SPECS" "$RUNBOOK_PROSE" "$ARCH" "$DEPLOY")"
if [ "$status" = "0" ]; then
  ok "prose explaining the absence of a migration is not read as one"
else
  bad "prose explaining the absence of a migration is not read as one" "exit $status: $(out)"
fi

RUNBOOK_NOSEC="$WORK_DIR/rb_nosection.md"
printf '# fixture\n\n## Evidence\n\nnone.\n' >"$RUNBOOK_NOSEC"
status="$(run_runner "$SPECS" "$RUNBOOK_NOSEC" "$ARCH" "$DEPLOY")"
if [ "$status" != "0" ] && out | grep -qF "no '## The one-move rollback' section"; then
  ok "a runbook with no rollback section fails rather than passing vacuously"
else
  bad "a runbook with no rollback section fails" "exit $status: $(out)"
fi

# --------------------------------------------------------------------------
# test_architecture_matches_deployed_metrics_path
# --------------------------------------------------------------------------
ARCH_CLAIMS="$WORK_DIR/arch_claims"
write_arch "$ARCH_CLAIMS" 'Fly.io managed Prometheus scrapes :9091/metrics off each machine.'
status="$(run_runner "$SPECS" "$RUNBOOK_OK" "$ARCH_CLAIMS" "$DEPLOY")"
if [ "$status" != "0" ] && out | grep -qF "the deployment does not declare"; then
  ok "an architecture doc claiming an undeployed scrape path fails"
else
  bad "an architecture doc claiming an undeployed scrape path fails" "exit $status: $(out)"
fi

# The same claim passes when the deployment actually declares the block: the
# rule is that documents and configuration agree, not that scraping is banned.
DEPLOY_WITH="$WORK_DIR/deploy_with"
write_deploy "$DEPLOY_WITH" yes
status="$(run_runner "$SPECS" "$RUNBOOK_OK" "$ARCH_CLAIMS" "$DEPLOY_WITH")"
if [ "$status" = "0" ]; then
  ok "a scrape claim matching a declared deployment block passes"
else
  bad "a scrape claim matching a declared deployment block passes" "exit $status: $(out)"
fi

# --------------------------------------------------------------------------
# test_runbook_has_no_orphan_owner_tag — the REAL runbook, not a fixture. A
# fill-tag ("`M…` fills / records / sets", "rows marked `M…`") names the
# milestone that OWNS those rows today, and the question this asks is whether
# that milestone still owns them.
#
# It is not "does a spec sit in active/". A milestone parked in done/ with
# unfinished Dimensions still owns its rows — that is what parked means — and
# keying on the directory failed the moment such a spec was filed. A milestone
# marked DONE does not: if its rows are still empty, either the fill never
# happened or the tag should name whoever inherited them. Both are the drift
# this catches, which is why a completed milestone is the failing case rather
# than a missing one.
#
# The id is read from the runbook, never written here (RULE TST-NAM), which is
# why no identifier appears above.
# --------------------------------------------------------------------------
orphan_tags=""
specs_dir="$SCRIPT_DIR/../../../docs/v2"
while IFS= read -r tag; do
  [ -n "$tag" ] || continue
  spec="$(find "$specs_dir" -maxdepth 2 -name "${tag}_*.md" 2>/dev/null | head -1)"
  if [ -z "$spec" ]; then
    orphan_tags="$orphan_tags $tag(no spec)"
    continue
  fi
  # `**Status:** DONE …` and `**Status:** ✅ DONE` both mean complete; PARKED,
  # DEFERRED and IN_PROGRESS all mean the rows are still somebody's.
  if grep -m1 '^\*\*Status:\*\*' "$spec" | grep -qE '^\*\*Status:\*\* (✅ )?DONE'; then
    orphan_tags="$orphan_tags $tag(complete)"
  fi
done < <(grep -oE '(`M[0-9]{3}_[0-9]{3}` (fills|records|sets)|rows marked `M[0-9]{3}_[0-9]{3}`)' \
           "$SCRIPT_DIR/001_playbook.md" | grep -oE 'M[0-9]{3}_[0-9]{3}' | sort -u)
if [ -z "$orphan_tags" ]; then
  ok "test_runbook_has_no_orphan_owner_tag"
else
  bad "test_runbook_has_no_orphan_owner_tag" "fill-tag names a milestone that no longer owns the rows:$orphan_tags"
fi

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ] || exit 1
[ "$passed" -gt 0 ] || { printf 'FAIL the self-test suite ran nothing\n' >&2; exit 1; }
