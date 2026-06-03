#!/usr/bin/env bash
# M7_002 Section 1: vault sync — verify credential fields exist and are consistent.
#
# Covers playbook sections 1.0 (Vercel bypass) and 2.0 (Upstash Redis passwords).

set -euo pipefail

echo ""
echo "== M7_002 Section 1: vault sync =="

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"

failures=0
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
    echo "  ✗ MISSING: $ref"
    failures=$((failures + 1))
  else
    echo "  ✓ $ref"
  fi
}

# --- 1.0 Vercel bypass secrets (PROD vault) ---
echo ""
echo "-- 1.0 Vercel bypass secrets (vault: $vault_prod)"

check_ref "op://$vault_prod/vercel-bypass-app/credential"
check_ref "op://$vault_prod/vercel-bypass-agents/credential"
check_ref "op://$vault_prod/vercel-bypass-website/credential"

# --- 2.0 Upstash DEV credentials (DEV vault) ---
echo ""
echo "-- 2.0 Upstash DEV credential sync (vault: $vault_dev)"

check_ref "op://$vault_dev/upstash-dev/url"
check_ref "op://$vault_dev/upstash-dev/api-url"
check_ref "op://$vault_dev/upstash-dev/worker-url"

# Read URLs for password sync verification
base_url="$(op_read_with_retry "op://$vault_dev/upstash-dev/url" || true)"
api_url="$(op_read_with_retry "op://$vault_dev/upstash-dev/api-url" || true)"
worker_url="$(op_read_with_retry "op://$vault_dev/upstash-dev/worker-url" || true)"

extract_pass() { echo "$1" | sed 's|rediss://[^:]*:\([^@]*\)@.*|\1|'; }

if [ -n "$base_url" ] && [ -n "$api_url" ] && [ -n "$worker_url" ]; then
  # Verify all three passwords match (compare hashes — never print values)
  hash_base="$(extract_pass "$base_url" | shasum -a 256)"
  hash_api="$(extract_pass "$api_url" | shasum -a 256)"
  hash_worker="$(extract_pass "$worker_url" | shasum -a 256)"

  if [ "$hash_base" = "$hash_api" ] && [ "$hash_base" = "$hash_worker" ]; then
    echo "  ✓ password hash matches across url, api-url, worker-url"
  else
    echo "  ✗ PASSWORD MISMATCH: url/api-url/worker-url passwords differ (hash comparison)"
    failures=$((failures + 1))
  fi

  # Verify api-url suffix
  if echo "$api_url" | grep -qE '/0\?role=api$'; then
    echo "  ✓ api-url ends with /0?role=api"
  else
    echo "  ✗ api-url does NOT end with /0?role=api"
    failures=$((failures + 1))
  fi

  # Verify worker-url suffix
  if echo "$worker_url" | grep -qE '/0\?role=worker$'; then
    echo "  ✓ worker-url ends with /0?role=worker"
  else
    echo "  ✗ worker-url does NOT end with /0?role=worker"
    failures=$((failures + 1))
  fi
else
  echo "  ! skipping password sync check — one or more URLs missing"
fi

# --- Result ---
echo ""
if [ "$failures" -gt 0 ]; then
  echo "section 1 failed: $failures issue(s) detected"
  exit 1
fi

echo "section 1 passed"
