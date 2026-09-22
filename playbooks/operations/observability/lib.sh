#!/usr/bin/env bash

set -euo pipefail

OBS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBS_REPO_ROOT="$(cd "$OBS_DIR/../../.." && pwd)"
# shellcheck source=../../../../lib/common.sh
source "$OBS_DIR/../../lib/common.sh"

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
    "op://$OBS_VAULT/agentsfleet-fleets-investigation-service-token/$field")"
  if [ -z "$value" ]; then
    echo "ERROR: missing $OBS_VAULT / agentsfleet-fleets-investigation-service-token / $field" >&2
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
  OBS_LOKI_UID="$(obs_read_required loki-datasource-uid)"
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

obs_get_loki_query_range() {
  local path="$1"
  local query="$2"
  curl --config "$OBS_CURL_CONFIG" \
    --header 'Accept: application/json' \
    --get \
    --data-urlencode "query=$query" \
    --data-urlencode 'since=15m' \
    --data-urlencode 'limit=1' \
    --data-urlencode 'direction=backward' \
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
  # `afd_core::timing`, not the Zig mirror it was ported from. The daemon that
  # publishes the heartbeat family and derives a runner offline is Rust:
  # `afd_runner/src/sweep/liveness.rs` tests `RUNNER_OFFLINE_AFTER_MS` from
  # here. Both files carry 30_000 today because a cross-runtime test pins the
  # Rust constants to the Zig ones, but that guard runs in the retired
  # runtime's direction, and the day `constants.zig` goes this playbook would
  # have derived its alert threshold from a deleted file.
  local constants="$OBS_REPO_ROOT/rustd/crates/afd_core/src/timing.rs"
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

obs_admission_replay_floor_seconds() {
  local replay="$OBS_REPO_ROOT/rustd/crates/afd_runner/src/sweep/replay.rs"
  local min_age interval
  min_age="$(
    sed -n 's/^const MIN_AGE: Duration = Duration::from_secs(\([0-9_]*\));/\1/p' \
      "$replay" | tr -d '_'
  )"
  interval="$(
    sed -n 's/^const INTERVAL: Duration = Duration::from_secs(\([0-9_]*\));/\1/p' \
      "$replay" | tr -d '_'
  )"
  # A row younger than MIN_AGE is deliberately left for its own producer, and
  # one older than that should be gone within a single INTERVAL pass. Their sum
  # is the age past which the sweeper is demonstrably behind, which is what the
  # census calls the replay floor.
  if [[ ! "$min_age" =~ ^[0-9]+$ ]] || [[ ! "$interval" =~ ^[0-9]+$ ]]; then
    echo "ERROR: cannot derive admission replay floor" >&2
    exit 1
  fi
  printf '%d' "$((min_age + interval))"
}

# Renders the dashboard asset into $1, resolving every placeholder.
#
# ONE renderer, called by both the apply and the drift check: the check proves
# the live dashboard matches what the apply would write, and it can only prove
# that if the two render identically. When these were two copies, a placeholder
# added to one made the other report drift that did not exist.
obs_render_dashboard() {
  local out="$1"
  local dir="$2"
  local numeric="$out.numeric"
  local offline replay_floor
  offline="$(obs_runner_offline_seconds)"
  replay_floor="$(obs_admission_replay_floor_seconds)"
  # The _NUM_ placeholders are quoted in the asset so it stays valid JSON, and
  # are unquoted here so a Grafana threshold step receives a number. jq's walk
  # rewrites string VALUES only, so a numeric field cannot be substituted
  # inside it — hence this text pass before the structural one.
  sed \
    -e "s/\"__RUNNER_OFFLINE_SECONDS_NUM__\"/$offline/g" \
    -e "s/\"__ADMISSION_REPLAY_FLOOR_SECONDS_NUM__\"/$replay_floor/g" \
    "$dir/assets/dashboard.json" >"$numeric"
  jq \
    --arg datasource "$OBS_PROMETHEUS_UID" \
    --arg environment "$OBS_ENVIRONMENT" \
    --arg dashboard "$OBS_DASHBOARD_NAME" \
    --arg offline "$offline" \
    --arg replay_floor "$replay_floor" \
    'walk(
      if type == "string" then
        gsub("__PROMETHEUS_UID__"; $datasource)
        | gsub("__ENVIRONMENT__"; $environment)
        | gsub("__DASHBOARD_UID__"; $dashboard)
        | gsub("__RUNNER_OFFLINE_SECONDS__"; $offline)
        | gsub("__ADMISSION_REPLAY_FLOOR_SECONDS__"; $replay_floor)
      else . end
    )' "$numeric" >"$out"
  rm -f "$numeric"
}

# Renders the alert asset into $1, resolving its one placeholder.
obs_render_alerts() {
  local out="$1"
  local dir="$2"
  jq --arg threshold "$(obs_runner_offline_seconds)" \
    'walk(
      if type == "string" then
        gsub("__RUNNER_OFFLINE_SECONDS__"; $threshold)
      else . end
    )' "$dir/assets/alerts.json" >"$out"
}
