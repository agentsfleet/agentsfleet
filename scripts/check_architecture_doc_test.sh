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

# Fixture milestones: one shipped, one in flight, one planned, one that exists
# nowhere. Workstream-suffixed names are composed rather than written out, since
# a literal `M<n>_<nnn>` in source is a milestone identifier the MS-ID gate bans
# (RULE TST-NAM) — tests are code, and the suffix is data here, not a reference.
readonly DONE_ID="M100"
readonly ACTIVE_ID="M200"
readonly PENDING_ONLY_ID="M777"
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

# Builds a spec tree: DONE_ID shipped, ACTIVE_ID in flight, PENDING_ONLY_ID planned.
build_spec_root() {
  local root="$1"
  mkdir -p "$root/done" "$root/active" "$root/pending"
  : >"$root/done/${DONE_ID}${WORKSTREAM}_P1_DONE_THING.md"
  : >"$root/active/${ACTIVE_ID}${WORKSTREAM}_P1_ACTIVE_THING.md"
  : >"$root/pending/${PENDING_ONLY_ID}${WORKSTREAM}_P1_PLANNED_THING.md"
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

# ── Dimension 4.3 — pending/ resolves in roadmap.md and nowhere else ─────────

test_arch_doc_roadmap_resolves_pending() {
  local name="test_arch_doc_roadmap_resolves_pending"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local roadmap elsewhere phantom_roadmap
  roadmap="$(build_arch_dir "$WORK_DIR/b1" roadmap.md "Depends on ${PENDING_ONLY_ID} (planned).")"
  if ! run_gate "$roadmap" "$spec_root"; then
    bad "$name" "roadmap.md must resolve a pending/-only milestone"
    return
  fi

  elsewhere="$(build_arch_dir "$WORK_DIR/b2" direction.md "Depends on ${PENDING_ONLY_ID} (planned).")"
  if run_gate "$elsewhere" "$spec_root"; then
    bad "$name" "a non-roadmap doc resolved a pending/-only milestone — the carve-out leaked"
    return
  fi

  # The carve-out widens where a spec may live, never whether one must exist.
  phantom_roadmap="$(build_arch_dir "$WORK_DIR/b3" roadmap.md "Depends on $PHANTOM_ID.")"
  if run_gate "$phantom_roadmap" "$spec_root"; then
    bad "$name" "roadmap.md laundered $PHANTOM_ID, which has no spec in any directory"
    return
  fi

  # The exemption is the top-level roadmap.md alone. A nested roadmap.md must not
  # inherit it — else any doc could launder unshipped ids by living at that name.
  local nested="$WORK_DIR/b4"
  mkdir -p "$nested/scenarios"
  printf '# nested\n\nDepends on %s (planned).\n' "$PENDING_ONLY_ID" >"$nested/scenarios/roadmap.md"
  if run_gate "$nested" "$spec_root"; then
    bad "$name" "a nested scenarios/roadmap.md resolved a pending-only milestone — basename carve-out leaked"
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

# ── The citation assertions catch a planted break ───────────────────────────
#
# One fixture per assertion, each planting exactly one bad citation in an
# otherwise-clean doc, so a non-zero exit can only come from that assertion. The
# gate must run against the repository root: the path, table and make-target
# checks read `git ls-files`, `schema/` and `make/`.

# Runs the gate from the repo root with a fixture ARCH_DIR and no extra doc set,
# so the live pages are never graded by a fixture case.
run_gate_from_root() {
  local arch_dir="$1" spec_root="$2"
  (cd "$REPO_ROOT" && ARCH_DIR="$arch_dir" SPEC_ROOT="$spec_root" DOC_SET_EXTRA="" \
    bash "$GATE" >/dev/null 2>&1)
}

# Asserts a clean body passes and a broken body fails, for one citation shape.
assert_citation_shape() {
  local name="$1" slug="$2" good_body="$3" bad_body="$4"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local clean broken
  clean="$(build_arch_dir "$WORK_DIR/${slug}_ok" direction.md "$good_body")"
  if ! run_gate_from_root "$clean" "$spec_root"; then
    bad "$name" "a resolvable citation was rejected: $good_body"
    return
  fi
  broken="$(build_arch_dir "$WORK_DIR/${slug}_bad" direction.md "$bad_body")"
  if run_gate_from_root "$broken" "$spec_root"; then
    bad "$name" "an unresolvable citation passed: $bad_body"
    return
  fi
  ok "$name"
}

test_arch_doc_cited_paths_resolve() {
  # The shorthand form (leading directories dropped) must keep resolving — the
  # pages use it throughout, and rejecting it would be a rewrite, not a check.
  assert_citation_shape test_arch_doc_cited_paths_resolve paths \
    'Reads `http/router.zig` and `schema/embed.zig`.' \
    'Reads `http/router_that_never_existed.zig`.'
}

test_arch_doc_cited_tables_exist() {
  assert_citation_shape test_arch_doc_cited_tables_exist tables \
    'Rows land in `core.fleet_events`.' \
    'Rows land in `core.table_that_never_existed`.'
}

test_arch_doc_cited_make_targets_exist() {
  assert_citation_shape test_arch_doc_cited_make_targets_exist targets \
    'Run `make lint-all`.' \
    'Run `make target-that-never-existed`.'
}

test_arch_doc_section_anchors_resolve() {
  local name="test_arch_doc_section_anchors_resolve"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  # Both fixture pages live in the same dir so the relative link resolves; the
  # target carries one heading, and only the second pointer names a missing one.
  local dir="$WORK_DIR/anchors"
  mkdir -p "$dir"
  printf '# Target\n\n## Real Section\n\nBody.\n' >"$dir/target.md"
  printf '# Source\n\nSee [`target.md`](./target.md) §Real Section.\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a pointer at an existing heading was rejected"
    return
  fi
  printf '# Source\n\nSee [`target.md`](./target.md) §Absent Section.\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a pointer at a heading the target lacks passed"
    return
  fi
  ok "$name"
}

test_arch_doc_validates_all_m_ids
test_arch_doc_roadmap_resolves_pending
test_arch_doc_unresolved_ref_names_the_milestone
test_arch_doc_missing_dir_fails_loud
test_arch_doc_wired_into_lint_all
test_arch_doc_real_corpus_resolves
test_arch_doc_cited_paths_resolve
test_arch_doc_cited_tables_exist
test_arch_doc_cited_make_targets_exist
test_arch_doc_section_anchors_resolve

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
