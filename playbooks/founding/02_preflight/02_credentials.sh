#!/usr/bin/env bash
set -euo pipefail

echo ""
echo "== 002_preflight Section 2: procurement readiness gate =="

env_mode="${ENV:-all}"
stage="${STAGE:-bootstrap}"
vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"

missing=0
declare -A OP_CACHE_VALUE
declare -A OP_CACHE_STATUS

case "$stage" in
  bootstrap|deployment) ;;
  *)
    echo "Unknown STAGE: $stage (supported: bootstrap, deployment)" >&2
    exit 2
    ;;
esac

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
  echo "-- checking PROD vault: $v ($stage)"

  # JWKS URL is derived from the issuer (the daemon builds
  # <issuer>/.well-known/jwks.json); the vault jwks-url field was removed.
  # OIDC_ISSUER is the single source of identity truth.
  check_url_ref "op://$v/clerk-prod/issuer"
  check_ref "op://$v/cloudflare-api-token/credential"
  check_ref "op://$v/npm-publish-token/credential"
  check_ref "op://$v/vercel-api-token/credential"
  check_ref "op://$v/vercel-bypass-website/credential"
  check_ref "op://$v/vercel-bypass-agents/credential"
  check_ref "op://$v/vercel-bypass-app/credential"
  check_ref "op://$v/posthog-prod/credential"
  check_ref "op://$v/clerk-prod/publishable-key"
  check_ref "op://$v/clerk-prod/secret-key"
  check_ref "op://$v/clerk-prod/webhook-secret"
  check_ref "op://$v/e2e-fixtures-email/regular"
  check_ref "op://$v/e2e-fixtures-email/admin"
  check_ref "op://$v/agentsfleet-admin/username"
  check_ref "op://$v/agentsfleet-admin/credential"
  check_ref "op://$v/encryption-master-key/credential"
  check_ref "op://$v/auth-session-code-pepper/credential"
  check_ref "op://$v/audit-log-pepper/credential"
  # Tailscale OAuth clients mint short-lived tagged access keys for CI and
  # persistent runner enrollment.
  check_ref "op://$v/tailscale/oauth-client-id"
  check_ref "op://$v/tailscale/oauth-secret"
  check_url_ref "op://$v/discord-ci-webhook/credential"
  check_url_ref "op://$v/discord-release-webhook/credential"
  check_ref "op://$v/fly-api-token/credential"

  if [ "$stage" = "deployment" ]; then
    # Browser callbacks bind workspace intent with this agentsfleet-owned
    # signing key. It is a Fly boot secret, not a provider app bag.
    check_ref "op://$v/approval-signing-secret/credential"
    check_ref "op://$v/cloudflare-r2/account-id"
    check_ref "op://$v/cloudflare-r2/access-key-id"
    check_ref "op://$v/cloudflare-r2/secret-access-key"
    check_ref "op://$v/cloudflare-r2/bucket"
    check_ref "op://$v/planetscale-prod/api-connection-string"
    check_ref "op://$v/planetscale-prod/migrator-connection-string"
    check_ref "op://$v/upstash-prod/api-url"
    check_ref "op://$v/upstash-prod/url"
    check_ref "op://$v/grafana-prod/otlp-endpoint"
    check_ref "op://$v/grafana-prod/instance-id"
    check_ref "op://$v/grafana-prod/api-key"
    check_ref "op://$v/cloudflare-tunnel-prod/credential"

    check_distinct \
      "op://$v/planetscale-prod/migrator-connection-string" \
      "op://$v/planetscale-prod/api-connection-string" \
      "prod postgres migrator vs api"
  fi

}

check_dev() {
  local v="$vault_dev"
  echo "-- checking DEV vault: $v ($stage)"

  # JWKS URL is derived from the issuer (the daemon builds
  # <issuer>/.well-known/jwks.json); the vault jwks-url field was removed.
  # OIDC_ISSUER is the single source of identity truth.
  check_url_ref "op://$v/clerk-dev/issuer"
  check_ref "op://$v/clerk-dev/publishable-key"
  check_ref "op://$v/clerk-dev/secret-key"
  check_ref "op://$v/clerk-dev/webhook-secret"
  check_ref "op://$v/e2e-fixtures-email/regular"
  check_ref "op://$v/e2e-fixtures-email/admin"
  check_ref "op://$v/agentsfleet-admin/username"
  check_ref "op://$v/agentsfleet-admin/credential"
  check_ref "op://$v/encryption-master-key/credential"
  check_ref "op://$v/auth-session-code-pepper/credential"
  check_ref "op://$v/audit-log-pepper/credential"
  check_ref "op://$v/posthog-dev/credential"
  check_ref "op://$v/fly-api-token/credential"

  if [ "$stage" = "deployment" ]; then
    check_ref "op://$v/approval-signing-secret/credential"
    check_ref "op://$v/planetscale-dev/api-connection-string"
    check_ref "op://$v/planetscale-dev/migrator-connection-string"
    check_ref "op://$v/upstash-dev/api-url"
    check_ref "op://$v/upstash-dev/url"
    check_ref "op://$v/grafana-dev/otlp-endpoint"
    check_ref "op://$v/grafana-dev/instance-id"
    check_ref "op://$v/grafana-dev/api-key"
    check_ref "op://$v/cloudflare-tunnel-dev/credential"
    check_ref "op://$v/cloudflare-r2/account-id"
    check_ref "op://$v/cloudflare-r2/access-key-id"
    check_ref "op://$v/cloudflare-r2/secret-access-key"
    check_ref "op://$v/cloudflare-r2/bucket"

    check_distinct \
      "op://$v/planetscale-dev/migrator-connection-string" \
      "op://$v/planetscale-dev/api-connection-string" \
      "dev postgres migrator vs api"
  fi

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
