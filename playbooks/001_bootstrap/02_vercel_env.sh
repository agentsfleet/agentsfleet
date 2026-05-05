#!/usr/bin/env bash
# Bootstrap section 2.7: Vercel project env-var sync.
#
# Reads PostHog + Clerk + API URL credentials from 1Password (vaults
# $VAULT_DEV / $VAULT_PROD) and upserts them on each Vercel project for
# both `preview` and `production` targets via the v10 env API.
#
# Run modes:
#   ./02_vercel_env.sh           # apply (POST upsert=true)
#   ./02_vercel_env.sh --check   # read-only diff, exit 1 on drift
#
# Why a script and not the playbook table alone: §2.7 prose has shipped
# half-done historically (PostHog rows missing on all three projects,
# observed via /v9/projects/{id}/env). A loud, idempotent script is the
# fix — re-running it is safe; skipping it now fails the preflight gate.

set -euo pipefail

mode="apply"
case "${1:-}" in
  --check) mode="check" ;;
  --apply|"") mode="apply" ;;
  *) echo "usage: $0 [--check|--apply]" >&2; exit 2 ;;
esac

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
posthog_host="${POSTHOG_HOST:-https://us.i.posthog.com}"
api_base="${VERCEL_API:-https://api.vercel.com}"

require_bin() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing tool: $1" >&2; exit 2; }
}
require_bin op
require_bin curl
require_bin jq

vercel_token="$(op read "op://$vault_prod/vercel-api-token/credential")"
[ -n "$vercel_token" ] || { echo "vercel-api-token empty" >&2; exit 2; }

# Project name → Vercel project ID. Resolved live so a new bootstrap
# (different team / renamed projects) doesn't need code changes.
declare -A PROJECT_ID
resolve_project() {
  local name="$1"
  local resp
  resp="$(curl -fsS -H "Authorization: Bearer $vercel_token" \
    "$api_base/v10/projects/$name")" || return 1
  PROJECT_ID["$name"]="$(echo "$resp" | jq -r '.id')"
}
for p in usezombie-website usezombie-agents-sh usezombie-app; do
  resolve_project "$p" || { echo "could not resolve project: $p" >&2; exit 2; }
done

# (project, key, prod-source, preview-source). Sources prefixed `op:` are
# 1Password refs resolved on apply; `lit:` is a literal value.
ROWS=(
  "usezombie-website|VITE_POSTHOG_KEY|op:op://$vault_prod/posthog-prod/credential|op:op://$vault_dev/posthog-dev/credential"
  "usezombie-website|VITE_POSTHOG_HOST|lit:$posthog_host|lit:$posthog_host"
  "usezombie-agents-sh|VITE_POSTHOG_KEY|op:op://$vault_prod/posthog-prod/credential|op:op://$vault_dev/posthog-dev/credential"
  "usezombie-agents-sh|VITE_POSTHOG_HOST|lit:$posthog_host|lit:$posthog_host"
  "usezombie-app|NEXT_PUBLIC_POSTHOG_KEY|op:op://$vault_prod/posthog-prod/credential|op:op://$vault_dev/posthog-dev/credential"
  "usezombie-app|NEXT_PUBLIC_POSTHOG_HOST|lit:$posthog_host|lit:$posthog_host"
)

resolve_source() {
  local src="$1"
  case "$src" in
    op:*) op read "${src#op:}" ;;
    lit:*) printf '%s' "${src#lit:}" ;;
    *) echo "bad source: $src" >&2; return 1 ;;
  esac
}

# `?decrypt=true` on the list endpoint is ignored for tokens without
# org-admin scope — values come back as base64 ciphertext (`eyJ2Ijoi…`).
# The per-env endpoint `/v1/projects/{id}/env/{envId}` returns plaintext
# for any token that can read the project. We list first to discover ids,
# then fetch plaintext per matching entry — ~2 extra round-trips per row,
# fine for a 12-row matrix.
fetch_envs() {
  local pid="$1"
  curl -fsS -H "Authorization: Bearer $vercel_token" \
    "$api_base/v9/projects/$pid/env?decrypt=false"
}

fetch_value() {
  local pid="$1" env_id="$2"
  curl -fsS -H "Authorization: Bearer $vercel_token" \
    "$api_base/v1/projects/$pid/env/$env_id" | jq -r '.value // empty'
}

drift=0
applied=0

for row in "${ROWS[@]}"; do
  IFS='|' read -r project key prod_src preview_src <<<"$row"
  pid="${PROJECT_ID[$project]}"

  prod_value="$(resolve_source "$prod_src")"
  preview_value="$(resolve_source "$preview_src")"

  current="$(fetch_envs "$pid")"
  prod_id="$(echo "$current" | jq -r --arg k "$key" \
    '.envs[] | select(.key==$k and (.target|index("production"))) | .id // empty')"
  preview_id="$(echo "$current" | jq -r --arg k "$key" \
    '.envs[] | select(.key==$k and (.target|index("preview"))) | .id // empty')"
  cur_prod=""; cur_preview=""
  [ -n "$prod_id" ] && cur_prod="$(fetch_value "$pid" "$prod_id")"
  [ -n "$preview_id" ] && cur_preview="$(fetch_value "$pid" "$preview_id")"

  for target in production preview; do
    if [ "$target" = "production" ]; then
      want="$prod_value"; have="$cur_prod"
    else
      want="$preview_value"; have="$cur_preview"
    fi

    if [ "$want" = "$have" ]; then
      echo "✓ $project/$key [$target]"
      continue
    fi

    if [ "$mode" = "check" ]; then
      if [ -z "$have" ]; then
        echo "✗ MISSING: $project/$key [$target]"
      else
        echo "✗ DRIFT: $project/$key [$target]"
      fi
      drift=$((drift + 1))
      continue
    fi

    payload="$(jq -nc \
      --arg k "$key" --arg v "$want" --arg t "$target" \
      '{key:$k, value:$v, type:"encrypted", target:[$t]}')"
    # v10 supports ?upsert=true — collapses create-or-update into one call.
    curl -fsS -X POST \
      -H "Authorization: Bearer $vercel_token" \
      -H "Content-Type: application/json" \
      -d "$payload" \
      "$api_base/v10/projects/$pid/env?upsert=true" >/dev/null
    echo "↑ upserted $project/$key [$target]"
    applied=$((applied + 1))
  done
done

if [ "$mode" = "check" ]; then
  if [ "$drift" -gt 0 ]; then
    echo ""
    echo "❌ $drift drift item(s) — re-run without --check to apply"
    exit 1
  fi
  echo ""
  echo "✅ vercel env in sync with vault"
  exit 0
fi

echo ""
echo "✅ vercel env applied — $applied write(s)"
echo "next: trigger a fresh redeploy per project (no build cache) so the"
echo "      new bundles inline the keys at vite/next build time."
