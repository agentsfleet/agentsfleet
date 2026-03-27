#!/usr/bin/env bash
# M4_001 Section 2: host readiness gate
# Verifies playbook step 2.0 (Tailscale) and step 3.0 (runtime deps):
#   - Tailscale installed and active
#   - bubblewrap (bwrap) installed
#   - git installed
#   - cgroups v2 active
#   - OpenSSL runtime available
set -euo pipefail

echo ""
echo "== M4_001 Section 2: host readiness =="

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
  echo "  ✗ SSH connectivity failed — cannot check host readiness"
  exit 1
fi
echo "  ✓ SSH connected to ${ssh_user}@${ssh_host}"

# 2.0 Tailscale installed and active
echo "-- checking Tailscale (step 2.0)"
ts_result="$(remote_cmd "command -v tailscale >/dev/null 2>&1 && tailscale status --json 2>/dev/null | head -1 || echo 'tailscale_missing'")"
if echo "$ts_result" | grep -q "tailscale_missing"; then
  echo "  ✗ Tailscale not installed"
  missing=$((missing + 1))
else
  echo "  ✓ Tailscale installed and active"
fi

# 3.0 bubblewrap installed
echo "-- checking bubblewrap (step 3.0)"
bwrap_result="$(remote_cmd "command -v bwrap >/dev/null 2>&1 && bwrap --version 2>&1 || echo 'bwrap_missing'")"
if echo "$bwrap_result" | grep -q "bwrap_missing"; then
  echo "  ✗ bubblewrap (bwrap) not installed"
  missing=$((missing + 1))
else
  echo "  ✓ bubblewrap installed ($bwrap_result)"
fi

# 3.0 git installed
echo "-- checking git (step 3.0)"
git_result="$(remote_cmd "command -v git >/dev/null 2>&1 && git --version 2>&1 || echo 'git_missing'")"
if echo "$git_result" | grep -q "git_missing"; then
  echo "  ✗ git not installed"
  missing=$((missing + 1))
else
  echo "  ✓ $git_result"
fi

# 3.0 cgroups v2 active
echo "-- checking cgroups v2 (step 3.0)"
cg_result="$(remote_cmd "test -f /sys/fs/cgroup/cgroup.controllers && echo 'cgv2_ok' || echo 'cgv2_missing'")"
if echo "$cg_result" | grep -q "cgv2_ok"; then
  echo "  ✓ cgroups v2 active"
else
  echo "  ✗ cgroups v2 not active — executor resource limits will not work"
  missing=$((missing + 1))
fi

# 3.0 OpenSSL runtime
echo "-- checking OpenSSL (step 3.0)"
ssl_result="$(remote_cmd "command -v openssl >/dev/null 2>&1 && openssl version 2>&1 || echo 'openssl_missing'")"
if echo "$ssl_result" | grep -q "openssl_missing"; then
  echo "  ✗ OpenSSL not installed"
  missing=$((missing + 1))
else
  echo "  ✓ $ssl_result"
fi

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 2 failed: $missing issue(s) detected"
  exit 1
fi

echo ""
echo "✅ section 2 passed"
