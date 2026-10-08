#!/usr/bin/env bash
# Run-directory fixtures shared by the host deploy suites.
#
#     source "$SCRIPT_DIR/deploy_test_support.sh"
#
# deploy_inputs_test.sh and deploy_run_dir_test.sh both hand deploy.sh a run
# directory as the runner playbook fills one. Building it in one place keeps
# the two from drifting apart when the run gains a file. Defines constants and
# functions only.

# What a run directory holds, as layout.sh names it, read in a subshell under
# other names: layout.sh's constants are readonly, and every case sources
# deploy.sh, which sources layout.sh again.
# shellcheck source=./layout.sh
read -r RUN_BINARY_FILE RUN_ENV_FILE RUN_NAME_PREFIX <<<"$(
  source "$(dirname "${BASH_SOURCE[0]}")/layout.sh" &&
    printf '%s ' "$BINARY_NAME" "$RUN_ENV_FILE_NAME" "$RUN_DIR_PREFIX"
)"
readonly RUN_BINARY_FILE RUN_ENV_FILE RUN_NAME_PREFIX
if [[ -z "$RUN_NAME_PREFIX" ]]; then
  printf 'FATAL: the run directory layout read empty from layout.sh\n' >&2
  exit 2
fi
readonly FIXTURE_DIGEST="0000000000000000000000000000000000000000000000000000000000000001"
readonly TOOLBOX_PARTS=(erofs json json.sig)
# The runner's environment as the playbook writes it: both keys deploy.sh
# requires, and a token that is not the placeholder.
readonly RUN_ENV_LINES=$'AGENTSFLEET_API_URL=https://api.example.test\nAGENTSFLEET_RUNNER_TOKEN=agt_rREAL'

# A directory holding the toolbox set, without `skip` if given.
toolbox_set() {
  local dir="$1" skip="${2:-}" part
  mkdir -p "$dir"
  for part in "${TOOLBOX_PARTS[@]}"; do
    [[ "$part" == "$skip" ]] || printf '%s\n' "$part" >"$dir/toolbox-$FIXTURE_DIGEST.$part"
  done
}

# A run directory as the playbook fills one, the binary, the toolbox set and
# the env file, without `skip` (a toolbox part, `binary` or `env`) if given.
# Any further arguments are added to the env file as lines.
run_dir_with() {
  local dir="$1" skip="${2:-}"
  shift
  [[ $# -eq 0 ]] || shift
  toolbox_set "$dir" "$skip"
  [[ "$skip" == binary ]] || printf 'runner\n' >"$dir/$RUN_BINARY_FILE"
  [[ "$skip" == env ]] || printf '%s\n' "$RUN_ENV_LINES" "$@" >"$dir/$RUN_ENV_FILE"
}
