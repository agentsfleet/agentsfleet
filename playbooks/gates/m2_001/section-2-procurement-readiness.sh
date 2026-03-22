#!/usr/bin/env bash
set -euo pipefail

echo ""
echo "== M2_001 Section 2: procurement readiness gate =="

env_mode="${ENV:-all}"
vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"

missing=0

check_ref() {
  local ref="$1"
  local value
  value="$(op read "$ref" 2>/dev/null || true)"
  if [ -z "$value" ]; then
    echo "✗ MISSING: $ref"
    missing=$((missing + 1))
  else
    echo "✓ $ref"
  fi
}

check_distinct() {
  local left_ref="$1"
  local right_ref="$2"
  local label="$3"

  local left right
  left="$(op read "$left_ref" 2>/dev/null || true)"
  right="$(op read "$right_ref" 2>/dev/null || true)"

  if [ -z "$left" ] || [ -z "$right" ]; then
    return
  fi

  if [ "$left" = "$right" ]; then
    echo "✗ INVALID: $label must differ"
    echo "  left:  $left_ref"
    echo "  right: $right_ref"
    missing=$((missing + 1))
  else
    echo "✓ distinct: $label"
  fi
}

check_prod() {
  local v="$vault_prod"
  echo "-- checking PROD vault: $v"

  check_ref "op://$v/cloudflare-api-token/credential"
  check_ref "op://$v/npm-publish-token/credential"
  check_ref "op://$v/vercel-bypass-website/credential"
  check_ref "op://$v/vercel-bypass-agents/credential"
  check_ref "op://$v/vercel-bypass-app/credential"
  check_ref "op://$v/posthog-prod/credential"
  check_ref "op://$v/clerk-prod/publishable-key"
  check_ref "op://$v/clerk-prod/secret-key"
  check_ref "op://$v/github-app/app-id"
  check_ref "op://$v/github-app/private-key"
  check_ref "op://$v/encryption-master-key/credential"
  check_ref "op://$v/planetscale-prod/api-connection-string"
  check_ref "op://$v/planetscale-prod/worker-connection-string"
  check_ref "op://$v/upstash-prod/api-url"
  check_ref "op://$v/upstash-prod/worker-url"
  check_ref "op://$v/tailscale/authkey"
  check_ref "op://$v/zombie-prod-worker-ant/ssh-private-key"
  check_ref "op://$v/zombie-prod-worker-bird/ssh-private-key"
  check_ref "op://$v/discord-ci-webhook/credential"
  check_ref "op://$v/fly-api-token/credential"
  check_ref "op://$v/posthog-prod/credential"
  check_ref "op://$v/cloudflare-tunnel-prod/credential"

  check_distinct \
    "op://$v/planetscale-prod/api-connection-string" \
    "op://$v/planetscale-prod/worker-connection-string" \
    "prod postgres api vs worker"

  check_distinct \
    "op://$v/upstash-prod/api-url" \
    "op://$v/upstash-prod/worker-url" \
    "prod redis api vs worker"
}

check_dev() {
  local v="$vault_dev"
  echo "-- checking DEV vault: $v"

  check_ref "op://$v/clerk-dev/publishable-key"
  check_ref "op://$v/clerk-dev/secret-key"
  check_ref "op://$v/github-app/app-id"
  check_ref "op://$v/github-app/private-key"
  check_ref "op://$v/encryption-master-key/credential"
  check_ref "op://$v/vercel-api-token/credential"
  check_ref "op://$v/posthog-dev/credential"
  check_ref "op://$v/planetscale-dev/api-connection-string"
  check_ref "op://$v/planetscale-dev/worker-connection-string"
  check_ref "op://$v/upstash-dev/api-url"
  check_ref "op://$v/upstash-dev/worker-url"
  check_ref "op://$v/fly-api-token/credential"
  check_ref "op://$v/posthog-dev/credential"
  check_ref "op://$v/cloudflare-tunnel-dev/credential"

  check_distinct \
    "op://$v/planetscale-dev/api-connection-string" \
    "op://$v/planetscale-dev/worker-connection-string" \
    "dev postgres api vs worker"

  check_distinct \
    "op://$v/upstash-dev/api-url" \
    "op://$v/upstash-dev/worker-url" \
    "dev redis api vs worker"
}

case "$env_mode" in
  all)
    check_prod
    check_dev
    ;;
  dev)
    check_dev
    ;;
  prod)
    check_prod
    ;;
  *)
    echo "Unknown ENV: $env_mode (supported: all, dev, prod)" >&2
    exit 2
    ;;
esac

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 2 failed: $missing issue(s) detected"
  exit 1
fi

echo ""
echo "✅ section 2 passed"
