#!/usr/bin/env bash
# M4_001 Section 1: SSH access gate
# Verifies playbook steps 1.0 and 3.0:
#   - Deploy SSH key exists in vault
#   - Hostname exists in vault
#   - Deploy user exists in vault
#   - SSH connectivity works via vault key + hostname
set -euo pipefail

echo ""
echo "== M4_001 Section 1: SSH access =="

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
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
    missing=$((missing + 1))
  else
    echo "  ✓ $ref"
  fi
}

echo "-- checking vault refs in: $vault_dev"

check_ref "op://$vault_dev/zombie-dev-worker-ant/ssh-private-key"
check_ref "op://$vault_dev/zombie-dev-worker-ant/hostname"
check_ref "op://$vault_dev/zombie-dev-worker-ant/tailscale-hostname"
check_ref "op://$vault_dev/zombie-dev-worker-ant/deploy-user"

# SSH connectivity test using vault key + tailscale-hostname
echo "-- checking SSH connectivity"
ssh_key="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/ssh-private-key" || true)"
ssh_host="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/tailscale-hostname" || true)"
ssh_user="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/deploy-user" || true)"

if [ -z "$ssh_key" ] || [ -z "$ssh_host" ] || [ -z "$ssh_user" ]; then
  echo "  ✗ SSH connectivity: skipped (missing vault refs)"
  missing=$((missing + 1))
else
  ssh_result="$(ssh -i <(printf '%s\n' "$ssh_key") \
    -o StrictHostKeyChecking=no \
    -o ConnectTimeout=10 \
    -o BatchMode=yes \
    "${ssh_user}@${ssh_host}" "echo ok" 2>&1 || true)"
  if [ "$ssh_result" = "ok" ]; then
    echo "  ✓ SSH connectivity: ${ssh_user}@${ssh_host}"
  else
    echo "  ✗ SSH connectivity failed: ${ssh_user}@${ssh_host}"
    echo "    output: $ssh_result"
    missing=$((missing + 1))
  fi
fi

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 1 failed: $missing issue(s) detected"
  exit 1
fi

echo ""
echo "✅ section 1 passed"
