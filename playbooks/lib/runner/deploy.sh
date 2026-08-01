#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"

validate_inputs() {
  RUNNER_BINARY="${RUNNER_BINARY:?RUNNER_BINARY must name the downloaded release binary}"
  RUNNER_VERSION="${RUNNER_VERSION:?RUNNER_VERSION must identify the source workflow or release}"

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
}

verify_host_prepared() {
  runner_remote '
    set -e
    test -d /opt/agentsfleet/bin
    test -d /opt/agentsfleet/deploy
    test -w /opt/agentsfleet/bin
    test -w /opt/agentsfleet/deploy
    command -v bwrap >/dev/null
    command -v nft >/dev/null
    command -v ip >/dev/null
    command -v curl >/dev/null
    command -v jq >/dev/null
  '
}

write_runner_environment() {
  local env_file
  env_file="$(mktemp)"
  trap 'rm -f "${env_file:-}"' RETURN
  {
    printf 'AGENTSFLEET_API_URL=%s\n' "$RUNNER_API_URL"
    printf 'AGENTSFLEET_RUNNER_TOKEN=%s\n' "$RUNNER_TOKEN"
  } >"$env_file"
  chmod 600 "$env_file"
  runner_copy "$env_file" /opt/agentsfleet/.env 600
  rm -f "$env_file"
  trap - RETURN
}

copy_deploy_files() {
  runner_copy \
    "$REPO_ROOT/deploy/baremetal/deploy.sh" \
    /opt/agentsfleet/deploy/deploy.sh \
    755
  runner_copy \
    "$REPO_ROOT/deploy/baremetal/agentsfleet-runner.service" \
    /opt/agentsfleet/deploy/agentsfleet-runner.service \
    644
  runner_copy "$RUNNER_BINARY" /opt/agentsfleet/bin/agentsfleet-runner 755
}

deploy_runner() {
  runner_remote "
    set -e
    sudo /opt/agentsfleet/deploy/deploy.sh runner '$RUNNER_VERSION' \
      /opt/agentsfleet/bin/agentsfleet-runner
  "
}

main() {
  runner_load_context
  validate_inputs

  echo "Deploying $RUNNER_ITEM in ${ENV} via Tailscale SSH"
  runner_verify_host_cgroup_capability
  verify_host_prepared
  copy_deploy_files
  write_runner_environment
  deploy_runner
  echo "PASS: $RUNNER_ITEM deployment completed"
}

main "$@"
