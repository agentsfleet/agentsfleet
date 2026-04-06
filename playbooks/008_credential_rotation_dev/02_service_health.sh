#!/usr/bin/env bash
# M7_002 Section 2: service health — verify services are healthy after rotation.
#
# Covers playbook section 4.0 (API health + Vercel bypass verification).

set -euo pipefail

echo ""
echo "== M7_002 Section 2: service health =="

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

# --- 4.0 API health checks ---
echo ""
echo "-- API health (api-dev.usezombie.com)"

if curl -sf --max-time 10 "https://api-dev.usezombie.com/healthz" >/dev/null 2>&1; then
  echo "  ✓ /healthz returned 200"
else
  echo "  ✗ /healthz did not return 200"
  failures=$((failures + 1))
fi

if curl -sf --max-time 10 "https://api-dev.usezombie.com/readyz" | jq -e '.ready == true' >/dev/null 2>&1; then
  echo "  ✓ /readyz reports ready=true"
else
  echo "  ✗ /readyz did not report ready=true"
  failures=$((failures + 1))
fi

# --- 4.0 Vercel bypass with rotated secret ---
echo ""
echo "-- Vercel bypass (usezombie-app.vercel.app)"

bypass_ref="op://$vault_prod/vercel-bypass-app/credential"
bypass_secret="$(op_read_with_retry "$bypass_ref" || true)"

if [ -z "$bypass_secret" ]; then
  echo "  ✗ cannot read bypass secret from vault — skipping bypass check"
  failures=$((failures + 1))
else
  http_code="$(curl -sf -o /dev/null -w '%{http_code}' --max-time 10 \
    -H "x-vercel-protection-bypass: $bypass_secret" \
    "https://usezombie-app.vercel.app/sign-in" 2>/dev/null || true)"

  if [ "$http_code" = "200" ]; then
    echo "  ✓ Vercel bypass returned 200"
  else
    echo "  ✗ Vercel bypass returned $http_code (expected 200)"
    failures=$((failures + 1))
  fi
fi

# --- Result ---
echo ""
if [ "$failures" -gt 0 ]; then
  echo "section 2 failed: $failures issue(s) detected"
  exit 1
fi

echo "section 2 passed"
