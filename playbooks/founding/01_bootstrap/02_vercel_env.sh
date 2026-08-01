#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

mode="check"
case "${1:-}" in
  --check | "") mode="check" ;;
  --apply) mode="apply" ;;
  *)
    echo "usage: $0 [--check|--apply]" >&2
    exit 2
    ;;
esac

playbooks_require_tool op
playbooks_require_tool curl
playbooks_require_tool jq
playbooks_require_vault_read_approval
playbooks_require_op_auth

if [ "$mode" = "apply" ] && [ "${ALLOW_VERCEL_WRITES:-0}" != "1" ]; then
  echo "ERROR: Vercel write approval required; set ALLOW_VERCEL_WRITES=1" >&2
  exit 1
fi

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
api_base="${VERCEL_API:-https://api.vercel.com}"
posthog_host="${POSTHOG_HOST:-https://us.i.posthog.com}"
work_dir="$(mktemp -d)"
auth_config="$work_dir/curl.conf"
owner_bash_pid="$BASHPID"
cleanup() {
  if [ "$BASHPID" = "$owner_bash_pid" ]; then
    rm -rf "$work_dir"
  fi
}
trap cleanup EXIT

vercel_token="$(op read "op://$vault_prod/vercel-api-token/credential")"
if [ -z "$vercel_token" ] ||
  [[ "$vercel_token" == *$'\n'* ]] ||
  [[ "$vercel_token" == *'"'* ]]; then
  echo "ERROR: Vercel token is missing or malformed" >&2
  exit 1
fi
umask 077
printf 'fail\nsilent\nshow-error\nheader = "Authorization: Bearer %s"\n' \
  "$vercel_token" >"$auth_config"
unset vercel_token

vercel_get() {
  local path="$1"
  curl --config "$auth_config" "$api_base$path"
}

declare -A PROJECT_ID
resolve_project() {
  local name="$1"
  local response
  response="$(vercel_get "/v10/projects/$name")" || return 1
  PROJECT_ID["$name"]="$(jq -r '.id // empty' <<<"$response")"
  [ -n "${PROJECT_ID[$name]}" ]
}

list_projects() {
  vercel_get "/v10/projects?limit=100" 2>/dev/null |
    jq -r '.projects[].name' |
    sort |
    sed 's/^/    - /' || true
}

for project in agentsfleet-website agentsfleet-app; do
  resolve_project "$project" && continue
  {
    echo "ERROR: Vercel project not found: $project"
    echo "Available projects:"
    list_projects
  } >&2
  exit 1
done

# Each row is project, key, production source, and preview source.
# `op:` values are resolved from 1Password; `lit:` values are public URLs.
rows=(
  "agentsfleet-website|VITE_APP_BASE_URL|lit:https://app.agentsfleet.net|lit:https://app-dev.agentsfleet.net"
  "agentsfleet-website|VITE_POSTHOG_KEY|op:op://$vault_prod/posthog-prod/credential|op:op://$vault_dev/posthog-dev/credential"
  "agentsfleet-website|VITE_POSTHOG_HOST|lit:$posthog_host|lit:$posthog_host"
  "agentsfleet-app|NEXT_PUBLIC_API_URL|lit:https://api.agentsfleet.net|lit:https://api-dev.agentsfleet.net"
  "agentsfleet-app|NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY|op:op://$vault_prod/clerk-prod/publishable-key|op:op://$vault_dev/clerk-dev/publishable-key"
  "agentsfleet-app|CLERK_SECRET_KEY|op:op://$vault_prod/clerk-prod/secret-key|op:op://$vault_dev/clerk-dev/secret-key"
  "agentsfleet-app|NEXT_PUBLIC_POSTHOG_KEY|op:op://$vault_prod/posthog-prod/credential|op:op://$vault_dev/posthog-dev/credential"
  "agentsfleet-app|NEXT_PUBLIC_POSTHOG_HOST|lit:$posthog_host|lit:$posthog_host"
)

resolve_source() {
  local source="$1"
  case "$source" in
    op:*) op read "${source#op:}" ;;
    lit:*) printf '%s' "${source#lit:}" ;;
    *)
      echo "ERROR: unsupported Vercel value source" >&2
      return 1
      ;;
  esac
}

fetch_envs() {
  local project_id="$1"
  vercel_get "/v9/projects/$project_id/env?decrypt=false"
}

fetch_value() {
  local project_id="$1"
  local env_id="$2"
  vercel_get "/v1/projects/$project_id/env/$env_id" |
    jq -r '.value // empty'
}

find_env_id() {
  local payload="$1"
  local key="$2"
  local target="$3"
  local count
  count="$(jq -r \
    --arg key "$key" \
    --arg target "$target" \
    '[.envs[] | select(.key == $key and (.target | index($target)))] | length' \
    <<<"$payload")"
  if [ "$count" -gt 1 ]; then
    echo "ERROR: duplicate Vercel rows for $key [$target]" >&2
    return 1
  fi
  jq -r \
    --arg key "$key" \
    --arg target "$target" \
    '.envs[] | select(.key == $key and (.target | index($target))) | .id' \
    <<<"$payload"
}

upsert_value() {
  local project_id="$1"
  local key="$2"
  local value="$3"
  local target="$4"
  local payload="$work_dir/upsert.json"
  jq -n \
    --arg key "$key" \
    --arg value "$value" \
    --arg target "$target" \
    '{key:$key,value:$value,type:"encrypted",target:[$target]}' >"$payload"
  curl --config "$auth_config" \
    --header 'Content-Type: application/json' \
    --request POST \
    --data-binary "@$payload" \
    "$api_base/v10/projects/$project_id/env?upsert=true" >/dev/null
}

drift=0
applied=0
for row in "${rows[@]}"; do
  IFS='|' read -r project key production_source preview_source <<<"$row"
  project_id="${PROJECT_ID[$project]}"
  production_value="$(resolve_source "$production_source")"
  preview_value="$(resolve_source "$preview_source")"
  current="$(fetch_envs "$project_id")"

  for target in production preview; do
    if [ "$target" = "production" ]; then
      wanted="$production_value"
    else
      wanted="$preview_value"
    fi
    env_id="$(find_env_id "$current" "$key" "$target")"
    actual=""
    [ -z "$env_id" ] || actual="$(fetch_value "$project_id" "$env_id")"

    if [ "$actual" = "$wanted" ]; then
      echo "OK: $project / $key [$target]"
      continue
    fi
    if [ "$mode" = "check" ]; then
      echo "DRIFT: $project / $key [$target]" >&2
      drift=$((drift + 1))
      continue
    fi

    upsert_value "$project_id" "$key" "$wanted" "$target"
    echo "UPDATED: $project / $key [$target]"
    applied=$((applied + 1))
  done
done

if [ "$mode" = "check" ] && [ "$drift" -ne 0 ]; then
  echo "ERROR: $drift Vercel value(s) drifted" >&2
  exit 1
fi

if [ "$mode" = "check" ]; then
  echo "PASS: Vercel environment values match 1Password"
else
  echo "PASS: Vercel environment values applied ($applied update(s))"
  echo "NEXT: redeploy changed projects without build cache"
fi
