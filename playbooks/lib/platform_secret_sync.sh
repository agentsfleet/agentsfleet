#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=playbooks/lib/common.sh
source "$script_dir/common.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool curl
playbooks_require_tool jq

if [ "${ALLOW_PLATFORM_SECRET_WRITES:-0}" != "1" ]; then
  echo "ERROR: platform-secret write approval required." >&2
  echo "Set ALLOW_PLATFORM_SECRET_WRITES=1 after Indy approves the target." >&2
  exit 1
fi

env_name="${ENV:-}"
secret_name="${1:-}"

case "$env_name" in
  dev)
    vault="${VAULT:-ZMB_CD_DEV}"
    api_base="https://api-dev.agentsfleet.net"
    ;;
  prod)
    vault="${VAULT:-ZMB_CD_PROD}"
    api_base="https://api.agentsfleet.net"
    ;;
  *)
    echo "ERROR: ENV must be dev or prod" >&2
    exit 2
    ;;
esac

declare -a field_refs
case "$secret_name" in
  github-app)
    field_refs=(
      "app_id|github-app/app_id"
      "app_slug|github-app/app_slug"
      "client_id|github-app/client_id"
      "client_secret|github-app/client_secret"
      "private_key_pem|github-app/private_key_pem"
      "webhook_secret|github-app/webhook_secret"
    )
    ;;
  slack-app)
    field_refs=(
      "client_id|slack-app/client_id"
      "client_secret|slack-app/client_secret"
      "signing_secret|slack-app/signing_secret"
    )
    ;;
  zoho-app|jira-app|linear-app)
    field_refs=(
      "client_id|$secret_name/client_id"
      "client_secret|$secret_name/client_secret"
    )
    ;;
  qstash)
    field_refs=(
      "token|qstash/token"
      "current_signing_key|qstash/current-signing-key"
      "next_signing_key|qstash/next-signing-key"
      "url|qstash/url"
    )
    ;;
  *)
    echo "ERROR: unsupported platform secret: ${secret_name:-<empty>}" >&2
    exit 2
    ;;
esac

api_key="$(op read "op://$vault/agentsfleet-admin/api-key")"
workspace_id="$(op read "op://$vault/agentsfleet-admin/platform_admin_workspace_id")"

if ! [[ "$api_key" =~ ^agt_t[0-9a-f]{64}$ ]]; then
  echo "ERROR: agentsfleet-admin/api-key is missing or malformed" >&2
  exit 1
fi
uuidv7_pattern='^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
if ! [[ "$workspace_id" =~ $uuidv7_pattern ]]; then
  echo "ERROR: agentsfleet-admin/platform_admin_workspace_id is not UUIDv7" >&2
  exit 1
fi

umask 077
work_dir="$(mktemp -d)"
pairs_file="$work_dir/pairs"
data_file="$work_dir/data.json"
payload_file="$work_dir/payload.json"
curl_config="$work_dir/curl.conf"
list_file="$work_dir/list.json"
response_file="$work_dir/response.json"

cleanup() {
  unset api_key workspace_id
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

printf 'fail\nsilent\nshow-error\n' >"$curl_config"
printf 'header = "Authorization: Bearer %s"\n' "$api_key" >>"$curl_config"
printf 'header = "Content-Type: application/json"\n' >>"$curl_config"
unset api_key

: >"$pairs_file"
for entry in "${field_refs[@]}"; do
  json_key="${entry%%|*}"
  item_field="${entry#*|}"
  value="$(op read "op://$vault/$item_field")"
  if [ -z "$value" ]; then
    echo "ERROR: empty 1Password field: op://$vault/$item_field" >&2
    exit 1
  fi
  printf '%s\0%s\0' "$json_key" "$value" >>"$pairs_file"
  unset value
done

jq -Rs '
  split("\u0000")
  | if .[-1] == "" then .[:-1] else . end
  | . as $parts
  | reduce range(0; ($parts | length); 2) as $index
      ({}; . + {($parts[$index]): $parts[$index + 1]})
' "$pairs_file" >"$data_file"

secrets_url="$api_base/v1/workspaces/$workspace_id/secrets"
curl --config "$curl_config" \
  --output "$list_file" \
  "$secrets_url"

if jq -e --arg name "$secret_name" \
  '(.secrets // []) | any(.name == $name)' "$list_file" >/dev/null; then
  method=PUT
  endpoint="$secrets_url/$secret_name"
  jq -n --slurpfile data "$data_file" \
    '{data: $data[0]}' >"$payload_file"
  outcome=replaced
else
  method=POST
  endpoint="$secrets_url"
  jq -n --arg name "$secret_name" --slurpfile data "$data_file" \
    '{name: $name, data: $data[0]}' >"$payload_file"
  outcome=created
fi

curl --config "$curl_config" \
  --request "$method" \
  --data-binary "@$payload_file" \
  --output "$response_file" \
  "$endpoint"

jq -e --arg name "$secret_name" '.name == $name' \
  "$response_file" >/dev/null
echo "✓ $secret_name $outcome in $env_name admin workspace"
