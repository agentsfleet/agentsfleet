#!/usr/bin/env bash

set -euo pipefail

RUNNER_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../common.sh
source "$RUNNER_LIB_DIR/../common.sh"

readonly CGROUP_ROOT="/sys/fs/cgroup"
readonly REQUIRED_CGROUP_CONTROLLERS="cpu memory pids"

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
  case "${ENV:-}" in
    dev)
      RUNNER_VAULT="${VAULT_DEV:-ZMB_CD_DEV}"
      RUNNER_ITEM="${WORKER_ITEM:-agentsfleet-dev-runner-ant}"
      RUNNER_API_URL="${AGENTSFLEET_API_URL:-https://api-dev.agentsfleet.net}"
      ;;
    prod)
      RUNNER_VAULT="${VAULT_PROD:-ZMB_CD_PROD}"
      RUNNER_ITEM="${WORKER_ITEM:?WORKER_ITEM is required for production}"
      RUNNER_API_URL="${AGENTSFLEET_API_URL:-https://api.agentsfleet.net}"
      ;;
    *)
      echo "ERROR: ENV must be dev or prod" >&2
      return 2
      ;;
  esac

  case "$RUNNER_API_URL" in
    https://*) ;;
    *)
      echo "ERROR: runner API URL must use HTTPS" >&2
      return 1
      ;;
  esac
  case "$RUNNER_API_URL" in
    *[!A-Za-z0-9:./_-]*)
      echo "ERROR: runner API URL contains unsupported characters" >&2
      return 1
      ;;
  esac
}

runner_load_target() {
  playbooks_require_op_auth
  playbooks_require_tool tailscale
  runner_select_environment

  RUNNER_HOST="$(runner_read_required "op://$RUNNER_VAULT/$RUNNER_ITEM/tailscale-hostname")"
  RUNNER_USER="$(runner_read_required "op://$RUNNER_VAULT/$RUNNER_ITEM/deploy-user")"
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
