#!/usr/bin/env bash
# Self-tests for the toolbox staging deploy.sh sources from toolbox.sh.
#
#     bash deploy/baremetal/toolbox_test.sh
#
# Each case sources deploy.sh in a fresh subshell, as deploy_test.sh does: its
# `readonly` constants can be assigned once per shell, and its `set -e` would
# abort on the non-zero returns these cases assert on. Staging writes only
# under this suite's scratch directory; every path is passed in.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPLOY_SH="$SCRIPT_DIR/deploy.sh"

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
readonly TOOLBOX_PARTS=(erofs json json.sig)

# A directory holding the toolbox set named `digest`, without `skip` if given.
toolbox_set() {
  local dir="$1" digest="$2" skip="${3:-}" part
  mkdir -p "$dir"
  for part in "${TOOLBOX_PARTS[@]}"; do
    [[ "$part" == "$skip" ]] || printf '%s\n' "$part" >"$dir/toolbox-$digest.$part"
  done
}

# Sources deploy.sh and stages `src` into `incoming` with the real `install`,
# not the stub, so the files land.
stage_toolbox() {
  local src="$1" incoming="$2"
  (
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    install_toolbox "$src" "$incoming" >/dev/null 2>&1
  )
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
  # shellcheck source=./deploy.sh
  set_home="$(source "$DEPLOY_SH" >/dev/null 2>&1; storage_home "$configured")"
  # shellcheck source=./deploy.sh
  default_home="$(source "$DEPLOY_SH" >/dev/null 2>&1; storage_home "$bare")"
  if [[ "$set_home" != "/srv/runner" ]]; then
    bad "$name" "a configured home read as '$set_home'"
  elif [[ "$default_home" != "/var/lib/agentsfleet-runner" ]]; then
    bad "$name" "an unset home read as '$default_home'"
  else
    ok "$name"
  fi
}

test_toolbox_staging_replaces_the_previous_set
test_toolbox_staging_refuses_an_incomplete_set
test_toolbox_storage_home_follows_the_env_file

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
