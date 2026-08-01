#!/usr/bin/env bash

set -euo pipefail

env_mode="${ENV:-all}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
api_base="https://api.vercel.com"
website_project="agentsfleet-website"
app_project="agentsfleet-app"
production_target="production"
preview_target="preview"
missing=0

case "$env_mode" in
  all | prod) ;;
  *)
    echo "ERROR: Vercel preflight supports ENV=all or ENV=prod" >&2
    exit 2
    ;;
esac

for required_tool in op curl jq; do
  if ! command -v "$required_tool" >/dev/null 2>&1; then
    echo "ERROR: required tool not found: $required_tool" >&2
    exit 1
  fi
done

read_vercel_token() {
  local ref="op://$vault_prod/vercel-api-token/credential"
  local attempts="${OP_READ_RETRIES:-2}"
  local delay_s="${OP_READ_BASE_DELAY_SECONDS:-1}"
  local min_interval_s="${OP_READ_MIN_INTERVAL_SECONDS:-0.2}"
  local attempt=1
  local value=""

  while [ "$attempt" -le "$attempts" ]; do
    sleep "$min_interval_s"
    if value="$(op read "$ref" 2>/dev/null)" && [ -n "$value" ]; then
      printf '%s' "$value"
      return 0
    fi
    if [ "$attempt" -lt "$attempts" ]; then
      sleep "$delay_s"
    fi
    attempt=$((attempt + 1))
  done
  return 1
}

vercel_token="$(read_vercel_token || true)"
if [ -z "$vercel_token" ]; then
  echo "ERROR: missing Vercel API token in $vault_prod" >&2
  exit 1
fi
if [[ "$vercel_token" == *$'\n'* ]] || [[ "$vercel_token" == *'"'* ]]; then
  echo "ERROR: Vercel API token is malformed" >&2
  exit 1
fi

work_dir="$(mktemp -d)"
curl_config="$work_dir/curl.conf"
cleanup() {
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

umask 077
printf 'fail\nsilent\nshow-error\nheader = "Authorization: Bearer %s"\n' \
  "$vercel_token" >"$curl_config"
unset vercel_token

vercel_get() {
  local path="$1"
  curl --config "$curl_config" "$api_base$path"
}

list_projects() {
  vercel_get "/v10/projects?limit=100" 2>/dev/null |
    jq -r '.projects[].name' |
    sort |
    sed 's/^/    - /' || true
}

check_project_envs() {
  local project="$1"
  shift
  local envs key targets

  if ! vercel_get "/v10/projects/$project" >/dev/null 2>&1; then
    echo "ERROR: Vercel project not found: $project" >&2
    echo "Available projects:" >&2
    list_projects >&2
    missing=$((missing + 1))
    return
  fi
  if ! envs="$(vercel_get "/v9/projects/$project/env?decrypt=false")"; then
    echo "ERROR: cannot read Vercel variables for $project" >&2
    missing=$((missing + 1))
    return
  fi

  for key in "$@"; do
    targets="$(jq -r --arg key "$key" \
      '[.envs[]? | select(.key == $key) | .target[]?] | unique | join(",")' \
      <<<"$envs")"
    if [[ ",$targets," == *",$production_target,"* ]] &&
      [[ ",$targets," == *",$preview_target,"* ]]; then
      echo "OK: Vercel $project / $key [$production_target+$preview_target]"
    else
      echo "MISSING: Vercel $project / $key (targets: ${targets:-none})" >&2
      missing=$((missing + 1))
    fi
  done
}

echo "== 002_preflight Section 3: Vercel environment inventory =="
check_project_envs "$website_project" \
  VITE_APP_BASE_URL \
  VITE_POSTHOG_KEY \
  VITE_POSTHOG_HOST
check_project_envs "$app_project" \
  NEXT_PUBLIC_API_URL \
  NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY \
  CLERK_SECRET_KEY \
  NEXT_PUBLIC_POSTHOG_KEY \
  NEXT_PUBLIC_POSTHOG_HOST

if [ "$missing" -ne 0 ]; then
  echo "ERROR: $missing Vercel environment issue(s) detected" >&2
  exit 1
fi

echo "PASS: Vercel variables exist for production and preview"
