#!/usr/bin/env bash
# Self-tests for toolbox.sh: what one toolbox set is, and staging it.
#
#     bash deploy/baremetal/toolbox_test.sh
#
# toolbox.sh takes every path as an argument and only returns on a refusal, so
# this suite sources it once and calls it directly. Staging writes only under
# this suite's scratch directory.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./toolbox.sh
source "$SCRIPT_DIR/toolbox.sh"

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# Digest-shaped names for the fixtures; staging reads them, nothing hashes them.
readonly FIXTURE_DIGEST="0000000000000000000000000000000000000000000000000000000000000001"
readonly OLDER_DIGEST="0000000000000000000000000000000000000000000000000000000000000002"
# The set the runner admits, spelled here rather than read from toolbox.sh, so
# a suffix dropped there fails these cases instead of shrinking their fixtures.
readonly TOOLBOX_PARTS=(erofs json json.sig)

# A directory holding the toolbox set named `digest`, without `skip` if given.
toolbox_set() {
  local dir="$1" digest="$2" skip="${3:-}" part
  mkdir -p "$dir"
  for part in "${TOOLBOX_PARTS[@]}"; do
    [[ "$part" == "$skip" ]] || printf '%s\n' "$part" >"$dir/toolbox-$digest.$part"
  done
}

# Stages `src` into `incoming`, quietly.
stage_toolbox() {
  install_toolbox "$1" "$2" >/dev/null 2>&1
}

# The set's paths, image first, so the playbook copies and the action bundles
# exactly what the host checks.
test_toolbox_set_files_names_the_three_files_image_first() {
  local name="test_toolbox_set_files_names_the_three_files_image_first"
  local dir="$WORK_DIR/named" got want part
  toolbox_set "$dir" "$FIXTURE_DIGEST"
  got="$(toolbox_set_files "$dir" | tr '\n' ' ')"
  want=""
  for part in "${TOOLBOX_PARTS[@]}"; do
    want+="$dir/toolbox-$FIXTURE_DIGEST.$part "
  done
  if [[ "$got" != "$want" ]]; then
    bad "$name" "toolbox_set_files printed [$got], want [$want]"
  elif toolbox_set_files "$WORK_DIR/no-such-dir" >/dev/null 2>&1; then
    bad "$name" "a directory with no image passed"
  else
    ok "$name"
  fi
}

# The staged set replaces the previous one whole, so the runner admits exactly
# the release this deploy brought.
test_toolbox_staging_replaces_the_previous_set() {
  local name="test_toolbox_staging_replaces_the_previous_set"
  local src="$WORK_DIR/stage-src" incoming="$WORK_DIR/stage-incoming" staged
  toolbox_set "$src" "$FIXTURE_DIGEST"
  toolbox_set "$incoming" "$OLDER_DIGEST"

  if ! stage_toolbox "$src" "$incoming"; then
    bad "$name" "a complete set was refused"
    return
  fi
  staged="$(cd "$incoming" && find . -type f | sort | tr '\n' ' ')"
  local want="./toolbox-$FIXTURE_DIGEST.erofs ./toolbox-$FIXTURE_DIGEST.json ./toolbox-$FIXTURE_DIGEST.json.sig "
  if [[ "$staged" != "$want" ]]; then
    bad "$name" "incoming holds [$staged], want [$want]"
  else
    ok "$name"
  fi
}

# A set short of any one file, or a directory with two images, is refused, and
# what was staged before stays as it was.
test_toolbox_staging_refuses_an_incomplete_set() {
  local name="test_toolbox_staging_refuses_an_incomplete_set"
  local incoming="$WORK_DIR/refuse-incoming" part n=0
  toolbox_set "$incoming" "$OLDER_DIGEST"
  for part in "${TOOLBOX_PARTS[@]}"; do
    n=$((n + 1))
    toolbox_set "$WORK_DIR/short-$n" "$FIXTURE_DIGEST" "$part"
    if stage_toolbox "$WORK_DIR/short-$n" "$incoming"; then
      bad "$name" "a set without its .$part was staged"
      return
    fi
  done
  toolbox_set "$WORK_DIR/two" "$FIXTURE_DIGEST"
  toolbox_set "$WORK_DIR/two" "$OLDER_DIGEST"
  if stage_toolbox "$WORK_DIR/two" "$incoming"; then
    bad "$name" "a directory with two images was staged"
  elif [[ ! -f "$incoming/toolbox-$OLDER_DIGEST.json.sig" ]]; then
    bad "$name" "a refused set disturbed the staged one"
  else
    ok "$name"
  fi
}

# The storage home is the env file's RUNNER_STORAGE_HOME when it sets one, and
# the runner's own default when it does not.
test_toolbox_storage_home_follows_the_env_file() {
  local name="test_toolbox_storage_home_follows_the_env_file"
  local configured="$WORK_DIR/configured.env" bare="$WORK_DIR/bare.env" set_home default_home
  printf 'AGENTSFLEET_API_URL=https://api.example.test\nRUNNER_STORAGE_HOME=/srv/runner\n' >"$configured"
  printf 'AGENTSFLEET_API_URL=https://api.example.test\n' >"$bare"
  set_home="$(storage_home "$configured")"
  default_home="$(storage_home "$bare")"
  if [[ "$set_home" != "/srv/runner" ]]; then
    bad "$name" "a configured home read as '$set_home'"
  elif [[ "$default_home" != "/var/lib/agentsfleet-runner" ]]; then
    bad "$name" "an unset home read as '$default_home'"
  else
    ok "$name"
  fi
}

test_toolbox_set_files_names_the_three_files_image_first
test_toolbox_staging_replaces_the_previous_set
test_toolbox_staging_refuses_an_incomplete_set
test_toolbox_storage_home_follows_the_env_file

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
