#!/usr/bin/env bash
# M4_001 Section 3: deploy readiness gate
# Verifies playbook step 6.0:
#   - /opt/zombie/deploy/deploy.sh exists and is executable
#   - /opt/zombie/deploy/zombied-executor.service exists
#   - /opt/zombie/deploy/zombied-worker.service exists
#   - /opt/zombie/.env exists with correct permissions (600)
#   - Systemd units installed in /etc/systemd/system/
set -euo pipefail

echo ""
echo "== M4_001 Section 3: deploy readiness =="

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

# Establish SSH connection details from vault
ssh_key="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/ssh-private-key" || true)"
ssh_host="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/hostname" || true)"
ssh_user="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/deploy-user" || true)"

if [ -z "$ssh_key" ] || [ -z "$ssh_host" ] || [ -z "$ssh_user" ]; then
  echo "  ✗ Cannot establish SSH — missing vault refs. Run section 1 first."
  exit 1
fi

remote_cmd() {
  ssh -i <(printf '%s\n' "$ssh_key") \
    -o StrictHostKeyChecking=no \
    -o ConnectTimeout=10 \
    -o BatchMode=yes \
    "${ssh_user}@${ssh_host}" "$@" 2>&1
}

# Verify SSH works before proceeding
if ! remote_cmd "echo ok" | grep -q "ok"; then
  echo "  ✗ SSH connectivity failed — cannot check deploy readiness"
  exit 1
fi
echo "  ✓ SSH connected to ${ssh_user}@${ssh_host}"

check_remote_file() {
  local path="$1"
  local label="$2"
  local expected_perms="${3:-}"

  local result
  result="$(remote_cmd "stat -c '%a %F' '$path' 2>/dev/null || echo 'NOT_FOUND'")"

  if echo "$result" | grep -q "NOT_FOUND"; then
    echo "  ✗ $label: $path not found"
    missing=$((missing + 1))
    return
  fi

  local perms
  perms="$(echo "$result" | awk '{print $1}')"

  if [ -n "$expected_perms" ] && [ "$perms" != "$expected_perms" ]; then
    echo "  ✗ $label: $path permissions $perms (expected $expected_perms)"
    missing=$((missing + 1))
    return
  fi

  echo "  ✓ $label: $path (perms: $perms)"
}

check_remote_executable() {
  local path="$1"
  local label="$2"

  local result
  result="$(remote_cmd "test -x '$path' && echo 'executable' || echo 'not_executable'")"

  if echo "$result" | grep -q "not_executable"; then
    echo "  ✗ $label: $path exists but not executable"
    missing=$((missing + 1))
    return
  fi

  echo "  ✓ $label: $path is executable"
}

# 6.1 Deploy artifacts
echo "-- checking deploy artifacts (step 6.1)"
check_remote_file "/opt/zombie/deploy/deploy.sh" "deploy script"
check_remote_executable "/opt/zombie/deploy/deploy.sh" "deploy script"
check_remote_file "/opt/zombie/deploy/zombied-executor.service" "executor unit"
check_remote_file "/opt/zombie/deploy/zombied-worker.service" "worker unit"

# 6.2 Environment file
echo "-- checking .env (step 6.2)"
check_remote_file "/opt/zombie/.env" "env file" "600"

# 6.3 Systemd units installed
echo "-- checking systemd units (step 6.3)"
check_remote_file "/etc/systemd/system/zombied-executor.service" "systemd executor unit"
check_remote_file "/etc/systemd/system/zombied-worker.service" "systemd worker unit"

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 3 failed: $missing issue(s) detected"
  exit 1
fi

echo ""
echo "✅ section 3 passed"
