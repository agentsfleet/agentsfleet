#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"

# The toolbox files a release or a lane build ships beside the binary: the
# image named by its digest, the release manifest, and cosign's signature over
# the manifest. The runner admits the three together and refuses leases without
# them, so a deploy that lacks any of them stops here.
readonly TOOLBOX_SUFFIXES=(erofs json json.sig)
# Where they sit: a directory of this name beside RUNNER_BINARY, as every
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
  toolbox_files
}

# Resolves the one toolbox under RUNNER_TOOLBOX_DIR into TOOLBOX_DIGEST and
# TOOLBOX_PATHS, refusing a directory with none, several, or a file short.
toolbox_files() {
  local images=("$RUNNER_TOOLBOX_DIR"/toolbox-*.erofs)
  if [ "${#images[@]}" -ne 1 ] || [ ! -f "${images[0]}" ]; then
    echo "ERROR: $RUNNER_TOOLBOX_DIR, beside the runner binary, must hold exactly one toolbox-<sha256>.erofs" >&2
    return 1
  fi
  local name
  name="$(basename "${images[0]}")"
  TOOLBOX_DIGEST="${name#toolbox-}"
  TOOLBOX_DIGEST="${TOOLBOX_DIGEST%.erofs}"
  TOOLBOX_PATHS=()
  local suffix
  for suffix in "${TOOLBOX_SUFFIXES[@]}"; do
    local file="$RUNNER_TOOLBOX_DIR/toolbox-$TOOLBOX_DIGEST.$suffix"
    [ -f "$file" ] || {
      echo "ERROR: toolbox file missing: $file" >&2
      return 1
    }
    TOOLBOX_PATHS+=("$file")
  done
}

verify_host_prepared() {
  runner_remote '
    set -e
    # Debian omits sbin from non-interactive Tailscale SSH sessions even
    # though nftables installs nft there. Match host preparation and the
    # established egress probe.
    export PATH="/usr/sbin:/sbin:$PATH"
    test -d /opt/agentsfleet/bin
    test -d /opt/agentsfleet/deploy
    test -w /opt/agentsfleet/bin
    test -w /opt/agentsfleet/deploy
    mkdir -p /opt/agentsfleet/toolbox
    test -w /opt/agentsfleet/toolbox
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
  # deploy.sh sources it from its own directory.
  runner_copy \
    "$REPO_ROOT/deploy/baremetal/toolbox.sh" \
    /opt/agentsfleet/deploy/toolbox.sh \
    644
  runner_copy \
    "$REPO_ROOT/deploy/baremetal/agentsfleet-runner.service" \
    /opt/agentsfleet/deploy/agentsfleet-runner.service \
    644
  runner_copy "$RUNNER_BINARY" /opt/agentsfleet/bin/agentsfleet-runner 755
  # The staging copy under /opt is the host's; deploy.sh moves the set into
  # the runner's incoming directory, where the runner admits it at boot.
  local file
  for file in "${TOOLBOX_PATHS[@]}"; do
    runner_copy "$file" "/opt/agentsfleet/toolbox/$(basename "$file")" 644
  done
}

deploy_runner() {
  runner_remote "
    set -e
    sudo /opt/agentsfleet/deploy/deploy.sh runner '$RUNNER_VERSION' \
      /opt/agentsfleet/bin/agentsfleet-runner /opt/agentsfleet/toolbox
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
