#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PLAYBOOKS_TEST_FORCE_GREP=1
# shellcheck source=test_search.sh
source "$SCRIPT_DIR/test_search.sh"

passed=0
failed=0
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

test_should_match_extended_regular_expressions() {
  local name="test_should_match_extended_regular_expressions"
  if printf '%s\n' 'POST request' | rg --quiet 'POST|PATCH'; then
    ok "$name"
  else
    bad "$name" "fallback did not match an extended regular expression"
  fi
}

test_should_match_fixed_strings() {
  local name="test_should_match_fixed_strings"
  if printf '%s\n' 'value+literal' | rg --fixed-strings --quiet 'value+literal'; then
    ok "$name"
  else
    bad "$name" "fallback did not match a fixed string"
  fi
}

test_should_count_and_list_matching_files() {
  local name="test_should_count_and_list_matching_files"
  printf '%s\n' 'needle' >"$work_dir/match"
  printf '%s\n' 'other' >"$work_dir/miss"
  if [ "$(rg -c needle "$work_dir/match")" -ne 1 ]; then
    bad "$name" "fallback returned the wrong count"
  elif ! rg -l needle "$work_dir/match" "$work_dir/miss" | grep -qx "$work_dir/match"; then
    bad "$name" "fallback did not list the matching file"
  else
    ok "$name"
  fi
}

test_should_search_directories_recursively() {
  local name="test_should_search_directories_recursively"
  mkdir -p "$work_dir/nested"
  printf '%s\n' 'needle' >"$work_dir/nested/match"
  if rg --quiet needle "$work_dir/nested"; then
    ok "$name"
  else
    bad "$name" "fallback did not search a directory recursively"
  fi
}

test_should_match_extended_regular_expressions
test_should_match_fixed_strings
test_should_count_and_list_matching_files
test_should_search_directories_recursively

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
