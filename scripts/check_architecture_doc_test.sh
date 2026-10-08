#!/usr/bin/env bash
# Self-tests for check_architecture_doc.sh's milestone-reference resolution.
#
#     bash scripts/check_architecture_doc_test.sh
#
# The gate drives fixture directories through ARCH_DIR + SPEC_ROOT. Each fixture
# architecture dir is built to pass the gate's other two checks (no relative .md
# links, no orphan markers), so a non-zero exit can only come from an unresolved
# milestone reference — otherwise these tests would pass for the wrong reason.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
readonly GATE="$SCRIPT_DIR/check_architecture_doc.sh"
readonly MAKE_DIR="$REPO_ROOT/make"
readonly QUALITY_MK="$MAKE_DIR/quality.mk"

# Fixture milestones: one shipped, one in flight, one that exists nowhere. Workstream-suffixed names are composed rather than written out, since
# a literal `M<n>_<nnn>` in source is a milestone identifier the MS-ID gate bans
# (RULE TST-NAM) — tests are code, and the suffix is data here, not a reference.
readonly DONE_ID="M100"
readonly ACTIVE_ID="M200"
readonly PHANTOM_ID="M999"
readonly WORKSTREAM="_001"

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# Builds a spec tree: DONE_ID shipped, ACTIVE_ID in flight.
build_spec_root() {
  local root="$1"
  mkdir -p "$root/done" "$root/active" "$root/pending"
  : >"$root/done/${DONE_ID}${WORKSTREAM}_P1_DONE_THING.md"
  : >"$root/active/${ACTIVE_ID}${WORKSTREAM}_P1_ACTIVE_THING.md"
}

# `body` lands in `filename` inside a fresh architecture dir. No relative links
# and no orphan markers, so only the milestone check can fail.
build_arch_dir() {
  local dir="$1" filename="$2" body="$3"
  mkdir -p "$dir"
  printf '# Architecture fixture\n\n%s\n' "$body" >"$dir/$filename"
  printf '%s' "$dir"
}

# Runs the gate against a fixture pair; echoes nothing, returns its exit status.
run_gate() {
  local arch_dir="$1" spec_root="$2"
  ARCH_DIR="$arch_dir" SPEC_ROOT="$spec_root" bash "$GATE" >/dev/null 2>&1
}

# ── Dimension 4.1 — every identifier is validated, none skipped ──────────────

test_arch_doc_validates_all_m_ids() {
  local name="test_arch_doc_validates_all_m_ids"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local phantom shipped high_id
  phantom="$(build_arch_dir "$WORK_DIR/a1" direction.md "Depends on $PHANTOM_ID.")"
  if run_gate "$phantom" "$spec_root"; then
    bad "$name" "$PHANTOM_ID has no spec anywhere yet the gate passed"
    return
  fi

  shipped="$(build_arch_dir "$WORK_DIR/a2" direction.md "Built on ${DONE_ID} and ${ACTIVE_ID}${WORKSTREAM}.")"
  if ! run_gate "$shipped" "$spec_root"; then
    bad "$name" "a done/ + active/ citation should resolve"
    return
  fi

  # The frozen alternation only validated M40..M51; anything outside that range
  # was silently skipped. A high identifier must now be checked like any other.
  high_id="$(build_arch_dir "$WORK_DIR/a3" direction.md "Depends on M121.")"
  if run_gate "$high_id" "$spec_root"; then
    bad "$name" "M121 has no spec in the fixture yet the gate passed — high ids are still skipped"
    return
  fi
  ok "$name"
}


# The unresolved-reference path builds its own diagnostic; a dangling variable
# there (it once expanded a renamed constant under `set -u`) would crash with an
# unbound-variable error instead of naming the offending milestone. Assert the
# real message reaches the operator.
test_arch_doc_unresolved_ref_names_the_milestone() {
  local name="test_arch_doc_unresolved_ref_names_the_milestone"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local arch output
  arch="$(build_arch_dir "$WORK_DIR/u1" direction.md "Depends on $PHANTOM_ID.")"
  output="$(ARCH_DIR="$arch" SPEC_ROOT="$spec_root" bash "$GATE" 2>&1)"

  if [[ "$output" == *"unbound variable"* ]]; then
    bad "$name" "the failure path crashed on an unbound variable instead of reporting the ref: $output"
    return
  fi
  if [[ "$output" != *"$PHANTOM_ID"* ]]; then
    bad "$name" "the failure message did not name the unresolved milestone $PHANTOM_ID: $output"
    return
  fi
  ok "$name"
}

# A moved or renamed docs tree must fail loud, not pass by finding nothing.
test_arch_doc_missing_dir_fails_loud() {
  local name="test_arch_doc_missing_dir_fails_loud"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  if run_gate "$WORK_DIR/does-not-exist" "$spec_root"; then
    bad "$name" "gate passed against a non-existent ARCH_DIR — a moved corpus reports green"
    return
  fi
  ok "$name"
}

# ── Dimension 4.2 — the gate actually runs ───────────────────────────────────

# A target defined but unreferenced is exactly the state this gate was in before:
# present on disk, invoked by nothing. Both halves are asserted — the definition
# (in any included make file) and the lint-all edge that actually runs it.
test_arch_doc_wired_into_lint_all() {
  local name="test_arch_doc_wired_into_lint_all"

  if ! grep -qrE '^check-architecture-doc:' "$MAKE_DIR"; then
    bad "$name" "no make file under $MAKE_DIR defines a check-architecture-doc target"
    return
  fi
  if ! grep -qE '^lint-all:.*check-architecture-doc' "$QUALITY_MK"; then
    bad "$name" "check-architecture-doc is not a prerequisite of lint-all — the gate never runs"
    return
  fi
  # The definition is worthless if the Makefile never includes the file it lives in.
  if ! grep -qE '^include make/' "$REPO_ROOT/Makefile"; then
    bad "$name" "root Makefile includes no make/*.mk"
    return
  fi
  ok "$name"
}

# ── Regression — the live corpus resolves under the unfrozen scan ────────────

test_arch_doc_real_corpus_resolves() {
  local name="test_arch_doc_real_corpus_resolves"
  local output

  if ! output="$(cd "$REPO_ROOT" && bash "$GATE" 2>&1)"; then
    bad "$name" "the gate fails on the repo's own architecture docs: $output"
    return
  fi
  # Guards against a vacuous pass: a scan that matched nothing also exits 0.
  if [[ "$output" != *"milestone references resolve"* ]]; then
    bad "$name" "gate passed without resolving any milestone reference: $output"
    return
  fi
  ok "$name"
}

# ── The citation assertions live beside this file ───────────────────────────
#
# Sourced, so their helpers and tests share this script's counters, WORK_DIR
# and fixture builders. Split at the length cap.
# shellcheck source=scripts/check_architecture_doc_test_citations.sh
. "$SCRIPT_DIR/check_architecture_doc_test_citations.sh"

test_arch_doc_validates_all_m_ids
test_arch_doc_unresolved_ref_names_the_milestone
test_arch_doc_missing_dir_fails_loud
test_arch_doc_wired_into_lint_all
test_arch_doc_real_corpus_resolves
test_arch_doc_no_retired_slot_numbers() {
  local name="test_arch_doc_no_retired_slot_numbers"
  # A published decision record keeps its own title, so link text is exempt while
  # the same number in prose is not. Both halves are asserted.
  assert_citation_shape "$name" slots \
    'Indexes live in `schema/620_runner_lease_indexes.sql`, per [Index audit — slots 033 & 034](https://example.invalid/a).' \
    'Indexes live in slot 033.'
}

test_arch_doc_carries_no_conflict_marker() {
  # The marker that shipped rode the END of a sentence, which is why the check
  # cannot be line-anchored. The good body proves prose mentioning a merge is
  # still fine; the bad body is the exact shape that reached the default branch.
  assert_citation_shape test_arch_doc_carries_no_conflict_marker marker \
    'The merge brought both halves in cleanly.' \
    'The merge brought both halves in cleanly. >>>>>>> origin/main'
}

test_arch_doc_every_clickable_link_form_is_checked() {
  # Markdown offers five clickable destinations and the corpus uses one. The
  # other four are checked so a link written tomorrow in a form nobody used
  # before is read rather than skipped — the hole a single inline pattern left.
  local name="test_arch_doc_every_clickable_link_form_is_checked"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"
  local n=0 body dir
  for body in \
    'See [x](https://docs.agentsfleet.net/errors/UZ-EXEC-012).' \
    'See [x](<https://docs.agentsfleet.net/errors/UZ-EXEC-012>).' \
    'See [x][t].

[t]: https://docs.agentsfleet.net/errors/UZ-EXEC-012' \
    'See <https://docs.agentsfleet.net/errors/UZ-EXEC-012>.' \
    'See <a href="https://docs.agentsfleet.net/errors/UZ-EXEC-012">x</a>.'
  do
    n=$((n + 1))
    dir="$(build_arch_dir "$WORK_DIR/form_$n" direction.md "$body")"
    if run_gate_from_root "$dir" "$spec_root"; then
      bad "$name" "clickable form $n reached an unpublished page and passed"
      return
    fi
  done
  ok "$name"
}

test_arch_doc_only_link_targets_are_checked() {
  # A docs URL is checked when a reader can click it, which in Markdown means it
  # is a link target. Bare text spelling the same URL is example output — the
  # CLI's rendered `see:` line and an RFC 7807 `type` member both do it — and is
  # not navigation. Written this way the rule needs no fence parser: the tilde
  # fence, the four-backtick block and the indentation limit all stop mattering,
  # because none of those shapes is link syntax.
  local name="test_arch_doc_only_link_targets_are_checked"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"
  local bare fenced linked

  bare="$(build_arch_dir "$WORK_DIR/link_bare" direction.md \
    'Rendered: see https://docs.agentsfleet.net/errors/UZ-EXEC-012 for detail.')"
  if ! run_gate_from_root "$bare" "$spec_root"; then
    bad "$name" "a bare docs URL was treated as a link"
    return
  fi

  fenced="$(build_arch_dir "$WORK_DIR/link_fenced" direction.md \
    'Rendered:

~~~text
see: https://docs.agentsfleet.net/errors/UZ-EXEC-012
~~~')"
  if ! run_gate_from_root "$fenced" "$spec_root"; then
    bad "$name" "a tilde-fenced docs URL was treated as a link"
    return
  fi

  linked="$(build_arch_dir "$WORK_DIR/link_real" direction.md \
    'See [the page](https://docs.agentsfleet.net/errors/UZ-EXEC-012).')"
  if run_gate_from_root "$linked" "$spec_root"; then
    bad "$name" "an unpublished link target passed"
    return
  fi
  ok "$name"
}

test_arch_doc_published_links_resolve() {
  # A pointer to the published set must name a page that exists there. The good
  # body names a real one; the bad body names a plausible page nobody wrote.
  assert_citation_shape test_arch_doc_published_links_resolve published \
    'User-facing: [the memory page](https://docs.agentsfleet.net/memory).' \
    'User-facing: [the memory page](https://docs.agentsfleet.net/concepts/memory-internals).'
}

# ── No page names a retired runner outside a dated row ──────────────────────
#
# The four pages the schedule-ownership check reads carry their required QStash
# sentences, so only the fifth page decides. A failing case must fail on the
# retired-runner assertion by name, or it could pass for the wrong reason.

build_schedule_arch_dir() {
  local dir="$1" body="$2"
  mkdir -p "$dir"
  printf '# Fixture\n\nQStash owns the clock.\n' >"$dir/data_flow.md"
  printf '# Fixture\n\nQStash owns the clock.\n' >"$dir/user_flow.md"
  printf '# Fixture\n\nA cron is synchronously registered with Upstash QStash.\n' >"$dir/high_level.md"
  printf '# Fixture\n\nCron triggers belong to Upstash QStash.\n' >"$dir/README.md"
  printf '# Fixture\n\n%s\n' "$body" >"$dir/capabilities.md"
  printf '%s' "$dir"
}

test_arch_doc_names_no_retired_runner() {
  local name="test_arch_doc_names_no_retired_runner"
  local check="architecture_names_no_retired_runner"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local n=0 body dir output
  for body in \
    'The sandbox is built by `afr_sandbox`.' \
    '| 2026-10-02 | Indy: "the Zig runner is the rollback" |'
  do
    n=$((n + 1))
    dir="$(build_schedule_arch_dir "$WORK_DIR/retired_ok_$n" "$body")"
    if ! run_gate_from_root "$dir" "$spec_root"; then
      bad "$name" "a page the check must pass was rejected: $body"
      return
    fi
  done
  n=0
  for body in \
    'A NullClaw child runs the turn.' \
    'zig build test runs the lane.'
  do
    n=$((n + 1))
    dir="$(build_schedule_arch_dir "$WORK_DIR/retired_bad_$n" "$body")"
    output="$(cd "$REPO_ROOT" && ARCH_DIR="$dir" SPEC_ROOT="$spec_root" DOC_SET_EXTRA="" \
      bash "$GATE" 2>&1)" && {
      bad "$name" "a page naming a retired runner passed: $body"
      return
    }
    if [[ "$output" != *"$check: $dir/capabilities.md:3 "* ]]; then
      bad "$name" "the failure did not name $check at capabilities.md:3: $output"
      return
    fi
  done
  ok "$name"
}

test_arch_doc_cited_paths_resolve
test_arch_doc_cited_tables_exist
test_arch_doc_names_no_retired_runner
test_arch_doc_cited_make_targets_exist
test_arch_doc_section_anchors_resolve
test_arch_doc_no_retired_slot_numbers
test_arch_doc_punctuated_anchor_is_checked
test_arch_doc_inside_link_anchor_is_checked
test_arch_doc_multi_anchor_and_sibling_dir_are_checked
test_arch_doc_same_page_anchor_is_checked
test_arch_doc_carries_no_conflict_marker
test_arch_doc_published_links_resolve
test_arch_doc_only_link_targets_are_checked
test_arch_doc_every_clickable_link_form_is_checked

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
