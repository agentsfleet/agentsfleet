#!/usr/bin/env bash
set -euo pipefail

echo ""
echo "== 002_preflight Section 2: procurement readiness gate =="

env_mode="${ENV:-all}"
stage="${STAGE:-bootstrap}"
vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"

missing=0

# The read-through cache, as three parallel lists rather than two associative
# arrays: bash 3.2 has no `declare -A`, and the gate reads a few dozen refs, so
# a linear scan is invisible next to the `op read` it exists to avoid. Status
# is kept beside the value because a ref that FAILED must not be retried on
# every later question about it — a wrong credential and an unreadable one are
# both answered once.
OP_CACHE_REFS=()
OP_CACHE_VALUES=()
OP_CACHE_STATUSES=()

# The cache slot holding `$1`, or nothing when it has never been read.
op_cache_index() {
  local wanted="$1" index=0
  while [ "$index" -lt "${#OP_CACHE_REFS[@]}" ]; do
    if [ "${OP_CACHE_REFS[index]}" = "$wanted" ]; then
      printf '%s' "$index"
      return 0
    fi
    index=$((index + 1))
  done
  return 1
}

# Files `$2`/`$3` under `$1`, replacing an earlier answer for the same ref.
op_cache_put() {
  local ref="$1" status="$2" value="$3" index
  if index="$(op_cache_index "$ref")"; then
    OP_CACHE_STATUSES[index]="$status"
    OP_CACHE_VALUES[index]="$value"
    return 0
  fi
  OP_CACHE_REFS+=("$ref")
  OP_CACHE_STATUSES+=("$status")
  OP_CACHE_VALUES+=("$value")
}

case "$stage" in
  bootstrap|deployment) ;;
  *)
    echo "Unknown STAGE: $stage (supported: bootstrap, deployment)" >&2
    exit 2
    ;;
esac

op_read_with_retry() {
  local ref="$1" cached
  if cached="$(op_cache_index "$ref")"; then
    if [ "${OP_CACHE_STATUSES[cached]}" = "ok" ]; then
      printf '%s' "${OP_CACHE_VALUES[cached]}"
      return 0
    fi
    return 1
  fi

  local attempts="${OP_READ_RETRIES:-2}"
  local delay_s="${OP_READ_BASE_DELAY_SECONDS:-1}"
  local min_interval_s="${OP_READ_MIN_INTERVAL_SECONDS:-0.2}"
  local value=""

  for attempt in $(seq 1 "$attempts"); do
    # A 0-duration sleep has no observable effect but still forks a process —
    # tests set OP_READ_MIN_INTERVAL_SECONDS=0 precisely to disable the pacing
    # delay, and paid a fork per ref-check anyway. Skip the fork when the
    # delay is a literal no-op; production's real 0.2s default is unaffected.
    [ "$min_interval_s" = "0" ] || sleep "$min_interval_s"
    if value="$(op read "$ref" 2>/dev/null)"; then
      op_cache_put "$ref" ok "$value"
      printf '%s' "$value"
      return 0
    fi

    if [ "$attempt" -lt "$attempts" ]; then
      sleep "$delay_s"
    fi
  done

  op_cache_put "$ref" err ""
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
  # Development workflows post CI verdicts through the shared production
  # community webhook rather than storing a duplicate in the development vault.
  check_url_ref "op://$vault_prod/discord-ci-webhook/credential"

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
