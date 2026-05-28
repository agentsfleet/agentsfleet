#!/usr/bin/env bash
# Section 4: provision the zombie-runner env file from vault.
# Idempotent. Writes /opt/zombie/.env on the dev worker host with the three
# vars the runner daemon requires (see deploy/baremetal/zombie-runner.service):
#   ZOMBIE_API_URL       — control-plane base URL (literal for dev)
#   ZOMBIE_RUNNER_TOKEN  — pre-minted zrn_ token (Option B; vault: runner-token)
#   RUNNER_HOST_ID       — stable machine identifier (vault: hostname)
#
# Pre-Option-B, the daemon self-registered with a zmb_t_ API key and minted
# its own zrn_. Post-Option-B (commit c1ac7343), the platform admin pre-mints
# the zrn_ via POST /v1/runners and stores it under runner-token; the daemon
# authenticates with it directly and never self-registers.
#
# A real zrn_ requires the platform-admin enrollment gate (this milestone)
# served by a live dev control plane. Until that's done, store a placeholder
# in 1Password (e.g. `zrn_FAKE_REPLACE_BEFORE_DEV_WORKER_READY_TRUE`) so the
# bootstrap structure verifies end-to-end; DEV_WORKER_READY must stay `false`
# until the placeholder is swapped for a real token.
set -euo pipefail

echo ""
echo "== Section 4: provision /opt/zombie/.env =="

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
api_url="${ZOMBIE_API_URL:-https://api-dev.usezombie.com}"
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

require_ref() {
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
require_ref "op://$vault_dev/zombie-dev-worker-ant/ssh-private-key"
require_ref "op://$vault_dev/zombie-dev-worker-ant/tailscale-hostname"
require_ref "op://$vault_dev/zombie-dev-worker-ant/deploy-user"
require_ref "op://$vault_dev/zombie-dev-worker-ant/hostname"
require_ref "op://$vault_dev/zombie-dev-worker-ant/runner-token"

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 4 failed: $missing missing vault ref(s)"
  echo "  add the missing fields to 1Password before retrying."
  echo "  runner-token may be a placeholder zrn_ until the platform-admin"
  echo "  enrollment gate is live; the structure must exist either way."
  exit 1
fi

ssh_key="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/ssh-private-key")"
ssh_host="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/tailscale-hostname")"
ssh_user="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/deploy-user")"
host_id="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/hostname")"
runner_token="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/runner-token")"

# Fail loud on a stale pre-Option-B token so a wrong shape is caught here, not
# at runtime as a confusing 401 loop on the dev host.
if [[ "$runner_token" != zrn_* ]]; then
  echo "  ✗ runner-token does not start with zrn_ (Option B contract)"
  echo "    got: $(printf '%s' "$runner_token" | head -c 5)... (truncated)"
  exit 1
fi

ssh_opts=(-o StrictHostKeyChecking=no -o ConnectTimeout=15 -o BatchMode=yes)
ssh_id=$(mktemp)
trap 'rm -f "$ssh_id"' EXIT
printf '%s\n' "$ssh_key" > "$ssh_id"
chmod 600 "$ssh_id"

echo "-- writing /opt/zombie/.env on ${ssh_user}@${ssh_host}"
# Build the env body locally, then scp; avoids passing the token on the SSH
# command line where it would land in process listings on the remote host.
env_local=$(mktemp)
cat > "$env_local" <<EOF
ZOMBIE_API_URL=${api_url}
ZOMBIE_RUNNER_TOKEN=${runner_token}
RUNNER_HOST_ID=${host_id}
EOF
chmod 600 "$env_local"
scp -i "$ssh_id" "${ssh_opts[@]}" "$env_local" \
  "${ssh_user}@${ssh_host}:/opt/zombie/.env"
rm -f "$env_local"
ssh -i "$ssh_id" "${ssh_opts[@]}" "${ssh_user}@${ssh_host}" \
  "chmod 600 /opt/zombie/.env"

echo "-- restarting zombie-runner.service"
ssh -i "$ssh_id" "${ssh_opts[@]}" "${ssh_user}@${ssh_host}" \
  "sudo /opt/zombie/deploy/deploy.sh runner provisioning /opt/zombie/bin/zombie-runner 2>&1 || sudo systemctl restart zombie-runner.service"
sleep 3
status=$(ssh -i "$ssh_id" "${ssh_opts[@]}" "${ssh_user}@${ssh_host}" \
  "systemctl is-active zombie-runner.service 2>&1 || true")

if [ "$status" = "active" ]; then
  echo "  ✓ zombie-runner.service is active"
else
  echo "  ✗ zombie-runner.service is not active (status: $status)"
  echo "    inspect: ssh ${ssh_user}@${ssh_host} journalctl -u zombie-runner --no-pager -n 30"
  exit 1
fi

echo ""
echo "✅ section 4 passed"
