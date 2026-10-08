#!/usr/bin/env bash

set -euo pipefail

RUNNER_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../common.sh
source "$RUNNER_LIB_DIR/../common.sh"

readonly CGROUP_ROOT="/sys/fs/cgroup"
# The runner's host probe refuses to start without each of these
# (`REQUIRED_CONTROLLERS` in rustd/crates/afr_sandbox/src/probe.rs), and the
# Delegate= line in deploy/baremetal/agentsfleet-runner.service names the same
# ones. No test compares the three lists, so a controller the probe gains is
# added to all three in one change.
readonly REQUIRED_CGROUP_CONTROLLERS="cpu io memory pids"
# An allowlisted sandbox reaches its registries through this host: its packets
# are forwarded from its own link out of the host's. The runner's boot probe
# reads the setting and reports egress unenforced without it.
readonly IPV4_FORWARD_PROC="/proc/sys/net/ipv4/ip_forward"
readonly IPV4_FORWARD_SYSCTL_FILE="/etc/sysctl.d/60-agentsfleet-runner.conf"
# What a runner host must have on PATH: bubblewrap for each lease's sandbox, nft
# and ip for its egress boundary, and the curl and jq the checks in this
# directory run on the host. Host preparation installs them.
readonly RUNNER_HOST_TOOLS="bwrap nft ip curl jq"
# Debian omits sbin from non-interactive Tailscale SSH sessions even though
# nftables installs nft there. Expands on the host, not here.
# shellcheck disable=SC2016
readonly RUNNER_REMOTE_SBIN_PATH='export PATH="/usr/sbin:/sbin:$PATH"'

runner_read_required() {
  local ref="$1"
  local value
  value="$(op read "$ref" 2>/dev/null || true)"
  if [ -z "$value" ]; then
    echo "ERROR: missing 1Password field: $ref" >&2
    return 1
  fi
  printf '%s' "$value"
}

runner_select_environment() {
  local expected_api_url
  case "${ENV:-}" in
    dev)
      RUNNER_VAULT="${VAULT_DEV:-ZMB_CD_DEV}"
      RUNNER_ITEM="${RUNNER_ITEM:-agentsfleet-dev-runner-ant}"
      expected_api_url="https://api-dev.agentsfleet.net"
      ;;
    prod)
      RUNNER_VAULT="${VAULT_PROD:-ZMB_CD_PROD}"
      RUNNER_ITEM="${RUNNER_ITEM:?RUNNER_ITEM is required for production}"
      expected_api_url="https://api.agentsfleet.net"
      ;;
    *)
      echo "ERROR: ENV must be dev or prod" >&2
      return 2
      ;;
  esac

  RUNNER_API_URL="${AGENTSFLEET_API_URL:-$expected_api_url}"
  if [ "$RUNNER_API_URL" != "$expected_api_url" ]; then
    echo "ERROR: runner API URL must match the $ENV endpoint" >&2
    return 1
  fi
}

runner_validate_reference_component() {
  local label="$1"
  local value="$2"
  if [[ ! "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
    echo "ERROR: invalid runner $label" >&2
    return 1
  fi
}

runner_validate_target() {
  if [ "${#RUNNER_USER}" -gt 32 ] ||
    [[ ! "$RUNNER_USER" =~ ^[a-z_][a-z0-9_-]*$ ]]; then
    echo "ERROR: invalid runner deploy user" >&2
    return 1
  fi
  if [ "${#RUNNER_HOST}" -gt 253 ] ||
    [[ ! "$RUNNER_HOST" =~ ^[A-Za-z0-9][A-Za-z0-9.-]*[A-Za-z0-9]$ ]] ||
    [[ "$RUNNER_HOST" == *..* ]]; then
    echo "ERROR: invalid runner Tailscale hostname" >&2
    return 1
  fi
}

runner_load_target() {
  playbooks_require_vault_read_approval
  playbooks_require_op_auth
  playbooks_require_tool tailscale
  runner_select_environment
  runner_validate_reference_component vault "$RUNNER_VAULT"
  runner_validate_reference_component item "$RUNNER_ITEM"

  RUNNER_HOST="$(runner_read_required "op://$RUNNER_VAULT/$RUNNER_ITEM/tailscale-hostname")"
  RUNNER_USER="$(runner_read_required "op://$RUNNER_VAULT/$RUNNER_ITEM/deploy-user")"
  runner_validate_target
  RUNNER_TARGET="$RUNNER_USER@$RUNNER_HOST"
}

runner_load_context() {
  runner_load_target
  RUNNER_TOKEN="$(runner_read_required "op://$RUNNER_VAULT/$RUNNER_ITEM/runner-token")"

  case "$RUNNER_TOKEN" in
    agt_rFAKE*)
      echo "ERROR: $RUNNER_ITEM still has a placeholder runner token" >&2
      return 1
      ;;
    agt_r*)
      case "$RUNNER_TOKEN" in
        *[!A-Za-z0-9._-]*)
          echo "ERROR: $RUNNER_ITEM runner token contains unsupported characters" >&2
          return 1
          ;;
      esac
      ;;
    *)
      echo "ERROR: $RUNNER_ITEM runner token has the wrong prefix" >&2
      return 1
      ;;
  esac
}

runner_remote() {
  local command="$1"
  tailscale ssh "$RUNNER_TARGET" "$command"
}

# Fails, naming the first one missing, unless every RUNNER_HOST_TOOLS tool is
# on the host's PATH.
runner_require_remote_tools() {
  runner_remote "
    $RUNNER_REMOTE_SBIN_PATH
    for tool in $RUNNER_HOST_TOOLS; do
      if ! command -v \"\$tool\" >/dev/null; then
        echo \"ERROR: required host tool missing: \$tool\" >&2
        exit 1
      fi
    done
  "
}

# Fails unless the host reports itself online on the tailnet.
runner_require_tailnet_online() {
  runner_remote "test \"\$(tailscale status --json | jq -r .Self.Online)\" = true"
}

runner_verify_host_cgroup_capability() {
  runner_remote "
    set -e
    if [ ! -f '$CGROUP_ROOT/cgroup.controllers' ]; then
      echo 'ERROR: cgroup v2 controller inventory is unavailable: $CGROUP_ROOT/cgroup.controllers' >&2
      exit 1
    fi
    for controller in $REQUIRED_CGROUP_CONTROLLERS; do
      if ! grep -qw \"\$controller\" '$CGROUP_ROOT/cgroup.controllers'; then
        echo \"ERROR: required cgroup v2 controller unavailable: \$controller\" >&2
        exit 1
      fi
    done
  " || {
    echo "ERROR: cgroup v2 controller check failed for $RUNNER_TARGET" >&2
    return 1
  }
}

# Turns IPv4 forwarding on where it survives a reboot, and reads it back.
# Preparation and every deploy run it, so a host prepared before forwarding was
# part of preparation gains it at its next deploy.
runner_enable_ipv4_forwarding() {
  runner_remote "
    set -e
    echo 'net.ipv4.ip_forward = 1' | sudo tee '$IPV4_FORWARD_SYSCTL_FILE' >/dev/null
    sudo sysctl -q -p '$IPV4_FORWARD_SYSCTL_FILE'
    test \"\$(cat '$IPV4_FORWARD_PROC')\" = 1
  "
}

runner_copy() {
  local source_path="$1"
  local destination_path="$2"
  local mode="$3"
  local temporary_path="${destination_path}.new"

  [ -f "$source_path" ] || {
    echo "ERROR: local source missing: $source_path" >&2
    return 1
  }

  tailscale ssh "$RUNNER_TARGET" \
    "umask 077; cat > '$temporary_path' && chmod '$mode' '$temporary_path' && mv '$temporary_path' '$destination_path'" \
    <"$source_path"
}
