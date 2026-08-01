#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

env_mode="${ENV:-all}"
api_base="https://api.planetscale.com/v1"
tmp_dir="$(mktemp -d)"
auth_file="$tmp_dir/authorization"
payload_file="$tmp_dir/payload.json"
trap 'rm -rf "$tmp_dir"' EXIT
chmod 700 "$tmp_dir"

read_required() {
  local ref="$1"
  local value
  value="$(playbooks_read_ref_or_empty "$ref")"
  [ -n "$value" ] || {
    echo "ERROR: missing 1Password field: $ref" >&2
    return 1
  }
  printf '%s' "$value"
}

api_request() {
  local method="$1"
  local url="$2"
  local payload="${3:-}"
  local -a args=(
    --fail --silent --show-error
    --request "$method"
    --header "@$auth_file"
    --header "Accept: application/json"
  )
  if [ -n "$payload" ]; then
    printf '%s' "$payload" >"$payload_file"
    args+=(--header "Content-Type: application/json" --data-binary "@$payload_file")
  fi
  curl "${args[@]}" "$url"
}

apply_env() {
  local label="$1"
  local vault="$2"
  local item="$3"
  local organization database token cidrs url response count entry_id payload

  organization="$(read_required "op://$vault/$item/organization")"
  database="$(read_required "op://$vault/$item/database")"
  token="$(read_required "op://$vault/$item/service-token")"
  cidrs="$(read_required "op://$vault/fly-egress-ips/cidrs")"
  playbooks_is_ipv4_cidr_json_array "$cidrs"

  case "$organization" in
    *[!A-Za-z0-9_-]*)
      echo "ERROR: unsupported PlanetScale organization or database slug" >&2
      return 2
      ;;
  esac
  case "$database" in
    *[!A-Za-z0-9_-]*)
      echo "ERROR: unsupported PlanetScale organization or database slug" >&2
      return 2
      ;;
  esac

  printf 'Authorization: Bearer %s\n' "$token" >"$auth_file"
  chmod 600 "$auth_file"
  url="$api_base/organizations/$organization/databases/$database/cidrs"
  response="$(api_request GET "$url?per_page=100")"
  count="$(printf '%s' "$response" | jq -er \
    '[.data[] | select((.schema // "") == "" and (.role // "") == "")] | length')"
  payload="$(jq -cn --argjson cidrs "$cidrs" '{cidrs:($cidrs | unique | sort),schema:"",role:""}')"

  case "$count" in
    0)
      response="$(api_request POST "$url" "$payload")"
      printf '%s' "$response" | jq -e '.id and (.cidrs | length > 0)' >/dev/null
      echo "APPLIED: created $label PlanetScale IP restriction"
      ;;
    1)
      entry_id="$(printf '%s' "$response" | jq -er \
        '.data[] | select((.schema // "") == "" and (.role // "") == "") | .id')"
      if printf '%s' "$response" | jq -e --argjson wanted "$cidrs" \
          '.data[]
           | select((.schema // "") == "" and (.role // "") == "")
           | ((.cidrs | unique | sort) == ($wanted | unique | sort))' >/dev/null; then
        echo "UNCHANGED: $label PlanetScale IP restriction"
      else
        response="$(api_request PATCH "$url/$entry_id" "$payload")"
        printf '%s' "$response" | jq -e '.id and (.cidrs | length > 0)' >/dev/null
        echo "APPLIED: updated $label PlanetScale IP restriction"
      fi
      ;;
    *)
      echo "ERROR: multiple unrestricted PlanetScale IP entries found for $label" >&2
      return 1
      ;;
  esac
}

[ "${ALLOW_PROVIDER_WRITES:-0}" = "1" ] || {
  echo "ERROR: provider write approval required. Set ALLOW_PROVIDER_WRITES=1." >&2
  exit 1
}
playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool curl
playbooks_require_tool jq

case "$env_mode" in
  all)
    apply_env development "${VAULT_DEV:-ZMB_CD_DEV}" planetscale-dev
    apply_env production "${VAULT_PROD:-ZMB_CD_PROD}" planetscale-prod
    ;;
  dev) apply_env development "${VAULT_DEV:-ZMB_CD_DEV}" planetscale-dev ;;
  prod) apply_env production "${VAULT_PROD:-ZMB_CD_PROD}" planetscale-prod ;;
  *)
    echo "ERROR: ENV must be all, dev, or prod" >&2
    exit 2
    ;;
esac

echo "PASS: PlanetScale IP restrictions applied"
