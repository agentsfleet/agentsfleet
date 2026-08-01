#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"

require_approvals() {
  playbooks_require_vault_read_approval
  if [ "${ALLOW_RUNNER_HOST_PREPARE:-0}" != "1" ]; then
    echo "ERROR: runner host preparation requires Human approval" >&2
    exit 1
  fi
}

install_host_dependencies() {
  runner_remote '
    set -e
    sudo apt-get update -qq
    sudo apt-get install -y --no-install-recommends \
      bubblewrap nftables iproute2 ca-certificates git openssl curl jq
  '
}

prepare_host_paths() {
  runner_remote "
    set -e
    sudo mkdir -p /opt/agentsfleet/bin /opt/agentsfleet/deploy
    sudo chown -R '$RUNNER_USER:$RUNNER_USER' /opt/agentsfleet
  "
}

verify_host_base() {
  runner_remote '
    set -e
    test "$(tailscale status --json | jq -r .Self.Online)" = true
    command -v bwrap >/dev/null
    command -v nft >/dev/null
    command -v ip >/dev/null
    command -v curl >/dev/null
    command -v jq >/dev/null
  '
}

main() {
  require_approvals
  runner_load_target

  echo "Preparing $RUNNER_ITEM in ${ENV} via Tailscale SSH"
  runner_verify_host_cgroup_capability
  install_host_dependencies
  prepare_host_paths
  verify_host_base
  echo "PASS: $RUNNER_ITEM host preparation completed"
}

main "$@"
