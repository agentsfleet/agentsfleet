#!/usr/bin/env bash
set -euo pipefail

echo ""
echo "== M2_001 Section 2: procurement readiness gate =="

env_mode="${ENV:-all}"
vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"

missing=0
declare -A OP_CACHE_VALUE
declare -A OP_CACHE_STATUS

op_read_with_retry() {
  local ref="$1"
  if [ -n "${OP_CACHE_STATUS[$ref]:-}" ]; then
    if [ "${OP_CACHE_STATUS[$ref]}" = "ok" ]; then
      printf '%s' "${OP_CACHE_VALUE[$ref]}"
      return 0
    fi
    return 1
  fi

  local attempts="${OP_READ_RETRIES:-2}"
  local delay_s="${OP_READ_BASE_DELAY_SECONDS:-1}"
  local min_interval_s="${OP_READ_MIN_INTERVAL_SECONDS:-0.2}"
  local value=""

  for attempt in $(seq 1 "$attempts"); do
    # Smooth request bursts to avoid API request spikes.
    sleep "$min_interval_s"
    if value="$(op read "$ref" 2>/dev/null)"; then
      OP_CACHE_STATUS["$ref"]="ok"
      OP_CACHE_VALUE["$ref"]="$value"
      printf '%s' "$value"
      return 0
    fi

    if [ "$attempt" -lt "$attempts" ]; then
      sleep "$delay_s"
    fi
  done

  OP_CACHE_STATUS["$ref"]="err"
  OP_CACHE_VALUE["$ref"]=""
  return 1
}

check_ref() {
  local ref="$1"
  local value
  value="$(op_read_with_retry "$ref" || true)"
  if [ -z "$value" ]; then
    echo "✗ MISSING: $ref"
    missing=$((missing + 1))
  else
    echo "✓ $ref"
  fi
}

check_url_ref() {
  local ref="$1"
  local value
  value="$(op_read_with_retry "$ref" || true)"
  if [ -z "$value" ]; then
    echo "✗ MISSING: $ref"
    missing=$((missing + 1))
  elif ! echo "$value" | grep -qE '^https://[^[:space:]]+$'; then
    echo "✗ INVALID URL: $ref"
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
  left="$(op_read_with_retry "$left_ref" || true)"
  right="$(op_read_with_retry "$right_ref" || true)"

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

  check_url_ref "op://$v/clerk-prod/jwks-url"
  check_url_ref "op://$v/clerk-prod/issuer"
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
  check_ref "op://$v/planetscale-prod/migrator-connection-string"
  check_ref "op://$v/upstash-prod/api-url"
  check_ref "op://$v/upstash-prod/worker-url"
  check_ref "op://$v/tailscale/authkey"
  check_ref "op://$v/zombie-prod-worker-ant/ssh-private-key"
  check_ref "op://$v/zombie-prod-worker-bird/ssh-private-key"
  check_ref "op://$v/discord-ci-webhook/credential"
  check_ref "op://$v/fly-api-token/credential"
  check_ref "op://$v/posthog-prod/credential"
  check_ref "op://$v/grafana-prod/otlp-endpoint"
  check_ref "op://$v/grafana-prod/instance-id"
  check_ref "op://$v/grafana-prod/api-key"
  check_ref "op://$v/cloudflare-tunnel-prod/credential"

  check_distinct \
    "op://$v/planetscale-prod/api-connection-string" \
    "op://$v/planetscale-prod/worker-connection-string" \
    "prod postgres api vs worker"
  check_distinct \
    "op://$v/planetscale-prod/migrator-connection-string" \
    "op://$v/planetscale-prod/api-connection-string" \
    "prod postgres migrator vs api"
  check_distinct \
    "op://$v/planetscale-prod/migrator-connection-string" \
    "op://$v/planetscale-prod/worker-connection-string" \
    "prod postgres migrator vs worker"

  check_distinct \
    "op://$v/upstash-prod/api-url" \
    "op://$v/upstash-prod/worker-url" \
    "prod redis api vs worker"
}

check_dev() {
  local v="$vault_dev"
  echo "-- checking DEV vault: $v"

  check_url_ref "op://$v/clerk-dev/jwks-url"
  check_url_ref "op://$v/clerk-dev/issuer"
  check_ref "op://$v/clerk-dev/publishable-key"
  check_ref "op://$v/clerk-dev/secret-key"
  check_ref "op://$v/github-app/app-id"
  check_ref "op://$v/github-app/private-key"
  check_ref "op://$v/encryption-master-key/credential"
  check_ref "op://$v/vercel-api-token/credential"
  check_ref "op://$v/posthog-dev/credential"
  check_ref "op://$v/planetscale-dev/api-connection-string"
  check_ref "op://$v/planetscale-dev/worker-connection-string"
  check_ref "op://$v/planetscale-dev/migrator-connection-string"
  check_ref "op://$v/upstash-dev/api-url"
  check_ref "op://$v/upstash-dev/worker-url"
  check_ref "op://$v/fly-api-token/credential"
  check_ref "op://$v/posthog-dev/credential"
  check_ref "op://$v/grafana-dev/otlp-endpoint"
  check_ref "op://$v/grafana-dev/instance-id"
  check_ref "op://$v/grafana-dev/api-key"
  check_ref "op://$v/cloudflare-tunnel-dev/credential"

  check_distinct \
    "op://$v/planetscale-dev/api-connection-string" \
    "op://$v/planetscale-dev/worker-connection-string" \
    "dev postgres api vs worker"
  check_distinct \
    "op://$v/planetscale-dev/migrator-connection-string" \
    "op://$v/planetscale-dev/api-connection-string" \
    "dev postgres migrator vs api"
  check_distinct \
    "op://$v/planetscale-dev/migrator-connection-string" \
    "op://$v/planetscale-dev/worker-connection-string" \
    "dev postgres migrator vs worker"

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
