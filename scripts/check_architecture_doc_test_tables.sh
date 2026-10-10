#!/usr/bin/env bash
# Table-assertion self-tests for check_architecture_doc.sh. Sourced by
# scripts/check_architecture_doc_test.sh, never run alone: it reads that
# script's ok/bad counters, WORK_DIR, GATE, REPO_ROOT, build_spec_root and
# run_gate_from_root.

# A catalog publishing two tools, the way afr_tools spells one.
write_tool_catalog() {
  local path="$1"
  cat >"$path" <<'RUST'
pub const SHELL: Entry = Entry::new("shell", Runtime::Sandbox);
pub const MESSAGE: Entry = Entry::new("message", Runtime::Supervisor);
pub const UNLISTED: Entry = Entry::new("unlisted", Runtime::Supervisor);

/// Every published tool.
pub const PUBLISHED: [&Entry; 2] = [
    &SHELL,
    &MESSAGE,
];
RUST
}

# A runner_execution.md whose Tool catalog table carries `rows`.
build_tool_table_dir() {
  local dir="$1" rows="$2"
  mkdir -p "$dir"
  # The backticks are Markdown code spans, not command substitution.
  # shellcheck disable=SC2016
  printf '# Runner execution\n\n## Tool catalog\n\n| Tool | Runtime |\n|---|---|\n%s\n\n## Sandbox engines\n\n| `shell`, `message`, `ghost` | not the tool table |\n' \
    "$rows" >"$dir/runner_execution.md"
  printf '%s' "$dir"
}

test_arch_doc_tool_table_matches_catalog() {
  local name="moved_rules_reject_seeded_violations[architecture_tool_table_matches_catalog]"
  local check="architecture_tool_table_matches_catalog"
  local spec_root="$WORK_DIR/specs" catalog="$WORK_DIR/catalog.rs" dir output
  build_spec_root "$spec_root"
  write_tool_catalog "$catalog"

  dir="$(build_tool_table_dir "$WORK_DIR/tools_ok" $'| `shell` | sandbox |\n| `message` | supervisor |')"
  if ! TOOL_CATALOG="$catalog" run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "a table naming exactly the published tools was rejected"
    return
  fi
  dir="$(build_tool_table_dir "$WORK_DIR/tools_shared_cell" $'| `message`, `shell` | either |')"
  if ! TOOL_CATALOG="$catalog" run_gate_from_root "$dir" "$spec_root"; then
    bad "$name" "two tools sharing one cell were not both read"
    return
  fi
  for rows in $'| `shell` | sandbox |' $'| `shell` | sandbox |\n| `message` | supervisor |\n| `unlisted` | supervisor |'; do
    dir="$(build_tool_table_dir "$WORK_DIR/tools_bad_$RANDOM" "$rows")"
    output="$(cd "$REPO_ROOT" && TOOL_CATALOG="$catalog" ARCH_DIR="$dir" SPEC_ROOT="$spec_root" DOC_SET_EXTRA="" \
      bash "$GATE" 2>&1)" && {
      bad "$name" "a table that disagrees with the catalog passed: $rows"
      return
    }
    if [[ "$output" != *"$check: "* ]]; then
      bad "$name" "the failure did not name $check: $output"
      return
    fi
  done
  ok "$name"
}

# The published corpus losing runner_execution.md fails the check by name; a
# fixture directory without it is not the corpus and is left alone.
test_arch_doc_tool_table_page_missing_from_the_corpus_fails() {
  local name="architecture_tool_table_matches_catalog[the published corpus without the page fails]"
  local check="architecture_tool_table_matches_catalog"
  local root="$WORK_DIR/corpus_root" spec_root="$WORK_DIR/specs" output
  build_spec_root "$spec_root"
  build_arch_dir "$root/docs/architecture" direction.md "No tool table here." >/dev/null
  output="$(cd "$root" && SPEC_ROOT="$spec_root" DOC_SET_EXTRA="" bash "$GATE" 2>&1)" && {
    bad "$name" "the published corpus without runner_execution.md passed"
    return
  }
  if [[ "$output" != *"$check: docs/architecture/runner_execution.md is missing"* ]]; then
    bad "$name" "the failure did not name the missing page: $output"
    return
  fi
  ok "$name"
}
