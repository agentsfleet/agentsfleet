#!/usr/bin/env bash
# Self-tests for layout.sh: the files the runner playbook copies to a host are
# the files deploy.sh needs there.
#
#     bash deploy/baremetal/layout_test.sh
#
# The playbook copies HOST_DEPLOY_FILES, and nothing else, into the host's
# deploy directory, and deploy.sh sources its libraries from its own directory.
# So each case copies those files into a directory of its own, as the host
# holds them, and sources deploy.sh from there in a fresh bash: a source that
# fails then stops it under its own `set -e`, and prints why.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly LAYOUT_SH="$SCRIPT_DIR/layout.sh"
readonly ENTRY_POINT="deploy.sh"

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# The file names in HOST_DEPLOY_FILES, one per line.
deploy_files() {
  (
    # shellcheck source=./layout.sh
    source "$LAYOUT_SH"
    local entry
    for entry in "${HOST_DEPLOY_FILES[@]}"; do
      printf '%s\n' "${entry%%:*}"
    done
  )
}

# Copies the listed files, less `omit` if given, into `dir` and sources
# deploy.sh from there. Prints whatever sourcing printed; succeeds only when it
# exited 0 and printed nothing.
source_from_copy() {
  local dir="$1" omit="${2:-}" file output
  mkdir -p "$dir"
  while IFS= read -r file; do
    [[ "$file" == "$omit" ]] || cp "$SCRIPT_DIR/$file" "$dir/$file" || return 1
  done < <(deploy_files)
  # shellcheck disable=SC2016  # $1 expands in the child bash, not here
  output="$(bash -c 'source "$1"' _ "$dir/$ENTRY_POINT" 2>&1)" || {
    printf '%s\n' "${output:-sourcing exited non-zero}"
    return 1
  }
  [[ -z "$output" ]] || { printf '%s\n' "$output"; return 1; }
}

test_layout_lists_every_file_deploy_sources() {
  local name="test_layout_lists_every_file_deploy_sources"
  local output
  if [[ -z "$(deploy_files)" ]]; then
    bad "$name" "HOST_DEPLOY_FILES read empty from $LAYOUT_SH"
  elif ! output="$(source_from_copy "$WORK_DIR/complete")"; then
    bad "$name" "deploy.sh does not source from a directory holding only HOST_DEPLOY_FILES: $output"
  else
    ok "$name"
  fi
}

# The other half: every library listed is one deploy.sh sources, so taking any
# one away breaks it. A list that kept a library deploy.sh stopped reading
# would pass the case above and fail here.
test_layout_lists_only_files_deploy_sources() {
  local name="test_layout_lists_only_files_deploy_sources"
  local file
  while IFS= read -r file; do
    [[ "$file" == "$ENTRY_POINT" || "$file" != *.sh ]] && continue
    if source_from_copy "$WORK_DIR/without-$file" "$file" >/dev/null; then
      bad "$name" "deploy.sh sources cleanly without $file, which HOST_DEPLOY_FILES still copies"
      return
    fi
  done < <(deploy_files)
  ok "$name"
}

test_layout_lists_every_file_deploy_sources
test_layout_lists_only_files_deploy_sources

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
