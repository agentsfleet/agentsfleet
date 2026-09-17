#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

env_mode="${ENV:-all}"
planetscale_api="https://api.planetscale.com/v1"
max_age_days="${MAX_ALLOWLIST_ATTESTATION_AGE_DAYS:-7}"
tmp_dir="$(mktemp -d)"
auth_file="$tmp_dir/authorization"
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

get_json() {
  local url="$1"
  curl --fail --silent --show-error \
    --request GET \
    --header "@$auth_file" \
    --header "Accept: application/json" \
    "$url"
}

list_cidrs() {
  local url="$1"
  local page=1
  local response next_page
  local pages_file="$tmp_dir/cidr-pages.jsonl"
  : >"$pages_file"

  while :; do
    response="$(get_json "$url?page=$page&per_page=100")"
    printf '%s\n' "$response" >>"$pages_file"
    next_page="$(printf '%s' "$response" | jq -er '
      if .next_page == null then "done"
      elif (.next_page | type) == "number" and .next_page > 0 then
        .next_page | tostring
      else error("invalid PlanetScale pagination response")
      end
    ')"
    [ "$next_page" = "done" ] && break
    [ "$next_page" -gt "$page" ] || {
      echo "ERROR: invalid PlanetScale pagination sequence" >&2
      return 1
    }
    page="$next_page"
  done

  jq -s '{data: [.[].data[]]}' "$pages_file"
}

verify_planetscale() {
  local label="$1"
  local vault="$2"
  local item="$3"
  local organization database token wanted response
  organization="$(read_required "op://$vault/$item/organization")"
  database="$(read_required "op://$vault/$item/database")"
  token="$(read_required "op://$vault/$item/service-token")"
  wanted="$(read_required "op://$vault/fly-egress-ips/cidrs")"

  printf 'Authorization: Bearer %s\n' "$token" >"$auth_file"
  chmod 600 "$auth_file"
  response="$(list_cidrs \
    "$planetscale_api/organizations/$organization/databases/$database/cidrs")"
  printf '%s' "$response" | jq -e --argjson wanted "$wanted" '
    [.data[] | select((.schema // "") == "" and (.role // "") == "")] as $entries
    | ($entries | length) == 1
      and (($entries[0].cidrs | unique | sort) == ($wanted | unique | sort))
  ' >/dev/null || {
    echo "FAIL: $label PlanetScale IP restrictions differ from Fly.io egress inventory" >&2
    return 1
  }
  echo "PASS: $label PlanetScale IP restrictions"
}


# PlanetScale only. The datastore's half of this verification is gone with the
# store it verified: a self-hosted Dragonfly cluster has no public endpoint, no
# provider allowlist to attest, and no management API to read one back from. It
# is reached over 6PN, which admits nothing from outside the private network.
verify_env() {
  local label="$1"
  local vault="$2"
  local database_item="$3"
  verify_planetscale "$label" "$vault" "$database_item"
}

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool curl
playbooks_require_tool jq
playbooks_require_tool python3

case "$env_mode" in
  all)
    verify_env development "${VAULT_DEV:-ZMB_CD_DEV}" planetscale-dev
    verify_env production "${VAULT_PROD:-ZMB_CD_PROD}" planetscale-prod
    ;;
  dev) verify_env development "${VAULT_DEV:-ZMB_CD_DEV}" planetscale-dev ;;
  prod) verify_env production "${VAULT_PROD:-ZMB_CD_PROD}" planetscale-prod ;;
  *)
    echo "ERROR: ENV must be all, dev, or prod" >&2
    exit 2
    ;;
esac

echo "PASS: provider IP restrictions verified"
