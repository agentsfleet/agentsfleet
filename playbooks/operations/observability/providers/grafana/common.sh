#!/usr/bin/env bash

set -euo pipefail

OBS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBS_REPO_ROOT="$(cd "$OBS_DIR/../../../../.." && pwd)"
# shellcheck source=../../../../lib/common.sh
source "$OBS_DIR/../../../../lib/common.sh"

obs_select_environment() {
  case "${OBS_ENV:-}" in
    dev)
      OBS_ENVIRONMENT="development"
      OBS_VAULT="${VAULT_DEV:-ZMB_CD_DEV}"
      ;;
    prod)
      OBS_ENVIRONMENT="production"
      OBS_VAULT="${VAULT_PROD:-ZMB_CD_PROD}"
      ;;
    *)
      echo "ERROR: OBS_ENV must be dev or prod" >&2
      exit 2
      ;;
  esac

  OBS_FOLDER_NAME="agentsfleet-$OBS_ENV"
  OBS_DASHBOARD_NAME="agentsfleet-runtime-$OBS_ENV"
}

obs_require_tools() {
  playbooks_require_tool curl
  playbooks_require_tool jq
  playbooks_require_tool op
}

obs_read_required() {
  local field="$1"
  local value
  value="$(playbooks_read_ref_or_empty \
    "op://$OBS_VAULT/grafana-observability/$field")"
  if [ -z "$value" ]; then
    echo "ERROR: missing $OBS_VAULT / grafana-observability / $field" >&2
    exit 1
  fi
  printf '%s' "$value"
}

obs_open_session() {
  obs_require_tools
  playbooks_require_vault_read_approval
  playbooks_require_op_auth

  OBS_GRAFANA_URL="$(obs_read_required grafana-url)"
  OBS_GRAFANA_TOKEN="$(obs_read_required grafana-sa-token)"
  OBS_NAMESPACE="$(obs_read_required grafana-namespace)"
  OBS_PROMETHEUS_UID="$(obs_read_required prometheus-datasource-uid)"
  OBS_GRAFANA_URL="${OBS_GRAFANA_URL%/}"

  if [[ "$OBS_GRAFANA_URL" != https://* ]] &&
    [ "${ALLOW_INSECURE_GRAFANA_URL:-0}" != "1" ]; then
    echo "ERROR: Grafana URL must use HTTPS" >&2
    exit 1
  fi
  if [[ "$OBS_GRAFANA_TOKEN" == *$'\n'* ]] ||
    [[ "$OBS_GRAFANA_TOKEN" == *'"'* ]]; then
    echo "ERROR: Grafana token contains unsupported characters" >&2
    exit 1
  fi
  if [[ ! "$OBS_NAMESPACE" =~ ^[a-z0-9][a-z0-9-]*$ ]]; then
    echo "ERROR: invalid Grafana namespace" >&2
    exit 1
  fi

  OBS_CURL_CONFIG="$(mktemp)"
  chmod 600 "$OBS_CURL_CONFIG"
  printf 'fail\nsilent\nshow-error\nheader = "Authorization: Bearer %s"\n' \
    "$OBS_GRAFANA_TOKEN" >"$OBS_CURL_CONFIG"
}

obs_close_session() {
  if [ -n "${OBS_CURL_CONFIG:-}" ] && [ -f "$OBS_CURL_CONFIG" ]; then
    rm -f "$OBS_CURL_CONFIG"
  fi
  unset OBS_GRAFANA_TOKEN
}

obs_require_write_approval() {
  if [ "${ALLOW_OBSERVABILITY_WRITES:-0}" != "1" ]; then
    echo "ERROR: observability write approval required; set ALLOW_OBSERVABILITY_WRITES=1" >&2
    exit 1
  fi
}

obs_get_json() {
  local path="$1"
  curl --config "$OBS_CURL_CONFIG" \
    --header 'Accept: application/json' \
    "$OBS_GRAFANA_URL$path"
}

obs_get_query() {
  local path="$1"
  local query="$2"
  curl --config "$OBS_CURL_CONFIG" \
    --header 'Accept: application/json' \
    --get \
    --data-urlencode "query=$query" \
    "$OBS_GRAFANA_URL$path"
}

obs_get_status() {
  local path="$1"
  local output_file="$2"
  curl --config "$OBS_CURL_CONFIG" \
    --header 'Accept: application/json' \
    --output "$output_file" \
    --write-out '%{http_code}' \
    "$OBS_GRAFANA_URL$path" || true
}

obs_write_json() {
  local method="$1"
  local path="$2"
  local input_file="$3"
  local output_file="$4"
  curl --config "$OBS_CURL_CONFIG" \
    --header 'Accept: application/json' \
    --header 'Content-Type: application/json' \
    --request "$method" \
    --data-binary "@$input_file" \
    --output "$output_file" \
    --write-out '%{http_code}' \
    "$OBS_GRAFANA_URL$path"
}

obs_runner_offline_seconds() {
  local constants="$OBS_REPO_ROOT/src/lib/common/constants.zig"
  local lease_ms multiplier
  lease_ms="$(
    sed -n \
      's/^pub const LEASE_TTL_MS: i64 = \([0-9_]*\);/\1/p' \
      "$constants" | tr -d '_'
  )"
  multiplier="$(
    sed -n \
      's/^pub const RUNNER_OFFLINE_AFTER_MS: i64 = LEASE_TTL_MS \* \([0-9]*\);/\1/p' \
      "$constants"
  )"
  if [[ ! "$lease_ms" =~ ^[0-9]+$ ]] ||
    [[ ! "$multiplier" =~ ^[0-9]+$ ]] ||
    [ $((lease_ms % 1000)) -ne 0 ]; then
    echo "ERROR: cannot derive runner offline threshold" >&2
    exit 1
  fi
  printf '%d' "$((lease_ms / 1000 * multiplier))"
}
