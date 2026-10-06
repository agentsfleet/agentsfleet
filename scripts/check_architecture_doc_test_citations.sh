#!/usr/bin/env bash
# Citation-assertion self-tests for check_architecture_doc.sh. Sourced by
# scripts/check_architecture_doc_test.sh, never run alone: it reads that
# script's ok/bad counters, WORK_DIR, GATE, REPO_ROOT and fixture builders.

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
    'Reads `runner/main.zig` and `schema/100_schemas.sql`.' \
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

# A punctuated anchor (`§C. EXECUTE`) used to truncate to `C` and then match any
# heading containing that letter, so the assertion reported green on a pointer it
# never checked. The anchor must survive extraction whole, and must match at the
# START of a heading rather than anywhere inside it.
test_arch_doc_punctuated_anchor_is_checked() {
  local name="test_arch_doc_punctuated_anchor_is_checked"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local dir="$WORK_DIR/anchor_punct"
  mkdir -p "$dir"
  printf '# Target\n\n## C. EXECUTE (lease to report)\n\nBody.\n' >"$dir/target.md"

  printf '# Source\n\nSee [`target.md`](./target.md) §"C. EXECUTE".\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a quoted punctuated anchor naming a real heading was rejected"
    return
  fi

  # The regression: truncation left `C`, which matched the heading as a substring.
  printf '# Source\n\nSee [`target.md`](./target.md) §"C. DESTROY".\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a punctuated anchor naming no heading passed — truncated to its first token again"
    return
  fi

  # Substring matching would also accept an anchor buried mid-heading.
  printf '# Source\n\nSee [`target.md`](./target.md) §"EXECUTE (lease to report)".\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a mid-heading substring passed — the match is not prefix-anchored"
    return
  fi
  ok "$name"
}

# The section can also sit inside the link text — [`target.md` §Section](./target.md).
# Both extractors used to require `§` AFTER the destination, so every pointer
# written this way was skipped: the target heading could be renamed or deleted and
# the assertion still reported green. Six live pointers in the corpus used it.
test_arch_doc_inside_link_anchor_is_checked() {
  local name="test_arch_doc_inside_link_anchor_is_checked"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local dir="$WORK_DIR/anchor_inside"
  mkdir -p "$dir"
  printf '# Target\n\n## Egress model — outbound only\n\n## C. EXECUTE (lease to report)\n\nBody.\n' >"$dir/target.md"

  printf '# Source\n\nSee [`target.md` §Egress model](./target.md) for the rule.\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "an inside-link anchor naming a real heading was rejected"
    return
  fi

  # The regression: this was never read at all, so it passed while pointing nowhere.
  printf '# Source\n\nSee [`target.md` §Ingress model](./target.md) for the rule.\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "an inside-link anchor naming no heading passed — it was never extracted"
    return
  fi

  # Quoting has to carry punctuation in this position too, or `C. EXECUTE`
  # truncates to `C` and prefix-matches any heading that happens to start with it.
  printf '# Source\n\nSee [`target.md` §"C. EXECUTE"](./target.md) for the rule.\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a quoted punctuated inside-link anchor naming a real heading was rejected"
    return
  fi

  printf '# Source\n\nSee [`target.md` §"C. DESTROY"](./target.md) for the rule.\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a quoted inside-link anchor naming no heading passed"
    return
  fi
  ok "$name"
}

# One link can carry several sections — §"B. TRIGGER" and §"C. EXECUTE" — and a
# section can point across directories with `../`. Grepping for a link with one
# anchor attached read the first and skipped the rest, and dropping the `../`
# resolved the target to a path that does not exist, which the gate skipped
# rather than checked. A too-short anchor is the third hole: it prefix-matches
# several headings, naming none of them, so ambiguity has to fail on its own.
test_arch_doc_multi_anchor_and_sibling_dir_are_checked() {
  local name="test_arch_doc_multi_anchor_and_sibling_dir_are_checked"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  # `sibling.md` sits one level up from the architecture dir, reached by `../`.
  local root="$WORK_DIR/anchor_multi"
  local dir="$root/arch"
  mkdir -p "$dir"
  printf '# Sibling\n\n## Runner token (`agt_r`)\n\nBody.\n' >"$root/sibling.md"
  printf '# Target\n\n## B. TRIGGER\n\n## C. EXECUTE\n\n## Config\n\n## Connection topology\n\nBody.\n' >"$dir/target.md"

  printf '# Source\n\nSee [`target.md`](./target.md) §"B. TRIGGER" and §"C. EXECUTE".\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a second anchor on one link was rejected though both headings exist"
    return
  fi

  # The regression: the second anchor went unread, so a missing heading passed.
  printf '# Source\n\nSee [`target.md`](./target.md) §"B. TRIGGER" and §"D. REPORT".\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a second anchor naming no heading passed — only the first was read"
    return
  fi

  # `§C` prefix-matches C. EXECUTE, Config and Connection topology: names none.
  printf '# Source\n\nSee [`target.md`](./target.md) §C.\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "an anchor matching three headings passed — ambiguity is not a resolution"
    return
  fi

  printf '# Source\n\nSee [`../sibling.md`](../sibling.md) §"Runner token".\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a sibling-directory anchor at a real heading was rejected"
    return
  fi

  # The regression: `../` was dropped, the target resolved nowhere, and the
  # gate skipped the entry instead of failing on the missing heading.
  printf '# Source\n\nSee [`../sibling.md`](../sibling.md) §"Absent heading".\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a sibling-directory anchor naming no heading passed — target resolved nowhere"
    return
  fi

  # A bare anchor must stop at a following link rather than swallowing it:
  # `§Flow 1 + [`x.md`](./x.md)` names a section here and a page there.
  printf '# Target\n\n## B. TRIGGER\n\n## C. EXECUTE\n\n## Config\n\n## Connection topology\n\nBody.\n' >"$dir/target.md"
  printf '# Source\n\n## Flow 1\n\nSee §Flow 1 [`target.md`](./target.md) §"C. EXECUTE".\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a bare anchor swallowed the link that followed it"
    return
  fi
  ok "$name"
}

# Most `§` references carry no link at all — they name a heading on their own
# page. None of them were read until the gate learned `@self`, which is why
# `§Webhook auth` survived four months after M102 renamed that heading. Three
# behaviours are locked here: a same-page reference resolves without a link, a
# quoted anchor survives the line wrap that hard-wrapped prose puts through it,
# and a link stops binding at the end of its sentence.
test_arch_doc_same_page_anchor_is_checked() {
  local name="test_arch_doc_same_page_anchor_is_checked"
  local spec_root="$WORK_DIR/specs"
  build_spec_root "$spec_root"

  local dir="$WORK_DIR/anchor_self"
  mkdir -p "$dir"
  printf '# Other\n\n## Failure recovery model\n\nBody.\n' >"$dir/other.md"

  printf '# Source\n\n## Per-request volume\n\nSized in §"Per-request volume".\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a same-page anchor naming a real heading was rejected"
    return
  fi

  # The regression: unlinked references were never extracted, so this passed.
  printf '# Source\n\n## Per-request volume\n\nSized in §"Absent section".\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a same-page anchor naming no heading passed — it was never extracted"
    return
  fi

  # Hard-wrapped prose splits a quoted anchor across the line break.
  printf '# Source\n\n## Per-request volume\n\nSized in §"Per-request\nvolume" as described.\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a quoted anchor wrapped across a line break was rejected"
    return
  fi

  # A link binds only to the end of its sentence. Past the full stop the bare
  # anchor is about this page again, and must not resolve against the link.
  printf '# Source\n\n## Per-request volume\n\nSee [`other.md`](./other.md) §"Failure recovery model". The figure in §"Per-request volume" was wrong.\n' >"$dir/direction.md"
  if ! run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a same-page anchor after a link+full-stop was bound to the linked page"
    return
  fi

  # ...and within one sentence it does still bind to the link.
  printf '# Source\n\n## Per-request volume\n\nSee [`other.md`](./other.md) §"Failure recovery model" and §"Absent there".\n' >"$dir/direction.md"
  if run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a second anchor in the link's own sentence stopped binding to it"
    return
  fi
  ok "$name"
}
