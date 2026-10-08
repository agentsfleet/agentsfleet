#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"
# Where HOST_DEPLOY_FILES come from, toolbox.sh among them.
HOST_DEPLOY_SOURCE_DIR="$(cd "$SCRIPT_DIR/../../../deploy/baremetal" && pwd)"
readonly HOST_DEPLOY_SOURCE_DIR

# What one toolbox set is (toolbox_set_files), as the host's deploy.sh checks
# it. The runner admits the set's three files together and refuses leases
# without them, so a deploy that lacks any of them stops here.
# shellcheck source=../../../deploy/baremetal/toolbox.sh
source "$HOST_DEPLOY_SOURCE_DIR/toolbox.sh"
# Where the set sits: a directory of this name beside RUNNER_BINARY, as every
# workflow that downloads the two lays them out.
readonly TOOLBOX_DIR_NAME="toolbox"

validate_inputs() {
  RUNNER_BINARY="${RUNNER_BINARY:?RUNNER_BINARY must name the downloaded release binary}"
  RUNNER_VERSION="${RUNNER_VERSION:?RUNNER_VERSION must identify the source workflow or release}"
  RUNNER_TOOLBOX_DIR="$(dirname "$RUNNER_BINARY")/$TOOLBOX_DIR_NAME"

  [ -f "$RUNNER_BINARY" ] || {
    echo "ERROR: runner binary missing: $RUNNER_BINARY" >&2
    return 1
  }
  case "$RUNNER_VERSION" in
    *[!A-Za-z0-9._-]* | "")
      echo "ERROR: RUNNER_VERSION contains unsupported characters" >&2
      return 2
      ;;
  esac
  local files file
  files="$(toolbox_set_files "$RUNNER_TOOLBOX_DIR")" || {
    echo "ERROR: $RUNNER_TOOLBOX_DIR, beside the runner binary, must hold one complete toolbox set" >&2
    return 1
  }
  TOOLBOX_PATHS=()
  while IFS= read -r file; do
    TOOLBOX_PATHS+=("$file")
  done <<<"$files"
}

# Preparation creates the staging directories and hands HOST_ROOT to the
# deploy user. Creating them again here, as that user, costs nothing on a
# prepared host, brings in one the layout gained since its preparation, and
# fails on a host never prepared, whose HOST_ROOT the deploy user cannot write.
verify_host_prepared() {
  runner_remote "
    set -e
    mkdir -p $HOST_STAGING_DIRS
    for dir in $HOST_STAGING_DIRS; do
      test -w \"\$dir\"
    done
  "
  runner_require_remote_tools
}

# The directory this deploy stages into on the host, under HOST_RUNS_DIR
# (layout.sh); empty until the host makes it.
RUN_DIR=""
# 1 once the host's deploy.sh holds RUN_DIR. That deploy removes the directory
# when it ends, and this script must not: a playbook cancelled mid-deploy
# leaves the host's deploy running, still reading it.
RUN_DIR_HANDED_OFF=0
# The runner's environment, written here before its copy to the host. It holds
# the runner token, so the EXIT trap removes it however the deploy ends: a copy
# that fails exits the shell under `set -e`, and a RETURN trap never fires then.
RUNNER_ENV_TEMP=""

remove_runner_env_temp() {
  [ -z "$RUNNER_ENV_TEMP" ] || rm -f "$RUNNER_ENV_TEMP"
  RUNNER_ENV_TEMP=""
}

# What a deploy that stops leaves behind: the env file here, and the run
# directory on the host until deploy.sh holds it.
clean_up() {
  remove_runner_env_temp
  if [ -n "$RUN_DIR" ] && [ "$RUN_DIR_HANDED_OFF" = 0 ]; then
    runner_remote "rm -rf '$RUN_DIR'" ||
      echo "WARN: could not remove $RUN_DIR from $RUNNER_TARGET" >&2
  fi
}
trap clean_up EXIT

# The deploy files are the host's one copy, which the verify lane and a
# release deploy read in place.
copy_deploy_files() {
  local entry file
  for entry in "${HOST_DEPLOY_FILES[@]}"; do
    file="${entry%%:*}"
    runner_copy "$HOST_DEPLOY_SOURCE_DIR/$file" "$HOST_DEPLOY_DIR/$file" "${entry##*:}"
  done
}

# Has the host make this deploy's run directory, and keeps its path in RUN_DIR.
# The path comes back over SSH and is spliced into later remote commands, so
# anything but HOST_RUNS_DIR/RUN_DIR_PREFIX and mktemp's alphanumeric suffix is
# refused.
make_run_dir() {
  local made suffix
  made="$(runner_remote "mktemp -d $HOST_RUNS_DIR/${RUN_DIR_PREFIX}XXXXXXXX")"
  suffix="${made#"$HOST_RUNS_DIR/$RUN_DIR_PREFIX"}"
  if [ "$suffix" = "$made" ] || [ -z "$suffix" ] || [[ "$suffix" == *[!A-Za-z0-9]* ]]; then
    echo "ERROR: the host made no run directory under $HOST_RUNS_DIR: '$made'" >&2
    return 1
  fi
  RUN_DIR="$made"
}

# Copies the binary and the toolbox set into the run directory, where the
# host's deploy.sh reads them and no other deploy writes.
stage_run() {
  local file
  runner_copy "$RUNNER_BINARY" "$RUN_DIR/$BINARY_NAME" 755
  for file in "${TOOLBOX_PATHS[@]}"; do
    runner_copy "$file" "$RUN_DIR/$(basename "$file")" 644
  done
}

write_runner_environment() {
  RUNNER_ENV_TEMP="$(mktemp)"
  {
    printf 'AGENTSFLEET_API_URL=%s\n' "$RUNNER_API_URL"
    printf 'AGENTSFLEET_RUNNER_TOKEN=%s\n' "$RUNNER_TOKEN"
  } >"$RUNNER_ENV_TEMP"
  chmod 600 "$RUNNER_ENV_TEMP"
  runner_copy "$RUNNER_ENV_TEMP" "$RUN_DIR/$RUN_ENV_FILE_NAME" 600
  remove_runner_env_temp
}

deploy_runner() {
  RUN_DIR_HANDED_OFF=1
  runner_remote "
    set -e
    sudo $HOST_DEPLOY_DIR/deploy.sh runner '$RUNNER_VERSION' $RUN_DIR
  "
}

main() {
  runner_load_context
  validate_inputs

  echo "Deploying $RUNNER_ITEM in ${ENV} via Tailscale SSH"
  runner_verify_host_cgroup_capability
  verify_host_prepared
  runner_enable_ipv4_forwarding
  copy_deploy_files
  make_run_dir
  stage_run
  write_runner_environment
  deploy_runner
  echo "PASS: $RUNNER_ITEM deployment completed"
}

main "$@"
