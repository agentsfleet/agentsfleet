#!/usr/bin/env bash
# Section 4: provision the agentsfleet-runner env file from vault.
# Idempotent. Writes /opt/agentsfleet/.env on the dev worker host with the three
# vars the runner daemon requires (see deploy/baremetal/agentsfleet-runner.service):
#   AGENTSFLEET_API_URL       — control-plane base URL (literal for dev)
#   AGENTSFLEET_RUNNER_TOKEN  — pre-minted agt_r token (Option B; vault: runner-token)
#   RUNNER_HOST_ID       — stable machine id; must equal the minted fleet host_id
#                          (vault: tailscale-hostname = agent-…-worker-ant)
#
# Pre-Option-B, the daemon self-registered with a agt_t API key and minted
# its own agt_r. Post-Option-B (commit c1ac7343), the platform admin pre-mints
# the agt_r via POST /v1/runners and stores it under runner-token; the daemon
# authenticates with it directly and never self-registers.
#
# A real agt_r is minted by a platform admin from the dashboard ("Add runner",
# POST /v1/runners) on the live dev control plane and stored under runner-token.
# This script requires that real token: it seeds both the source /opt/agentsfleet/.env
# (which deploy.sh copies on every CI run) and the unit's EnvironmentFile
# /etc/default/agentsfleet-runner. When the binary exists it restarts and verifies
# the daemon; otherwise it stops the retry loop and leaves startup to deploy.sh.
# A agt_rFAKE_… placeholder is rejected below; DEV_WORKER_READY stays `false`
# until a real token brings the runner up green.
set -euo pipefail

echo ""
echo "== Section 4: provision /opt/agentsfleet/.env =="

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
api_url="${AGENTSFLEET_API_URL:-https://api-dev.agentsfleet.net}"
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
require_ref "op://$vault_dev/zombie-dev-worker-ant/runner-token"

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 4 failed: $missing missing vault ref(s)"
  echo "  add the missing fields to 1Password before retrying."
  echo "  runner-token may be a placeholder agt_r until the platform-admin"
  echo "  enrollment gate is live; the structure must exist either way."
  exit 1
fi

ssh_key="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/ssh-private-key")"
ssh_host="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/tailscale-hostname")"
ssh_user="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/deploy-user")"
# RUNNER_HOST_ID must equal the fleet host_id the admin minted (POST /v1/runners):
# the tailscale hostname (agent-…-worker-ant). Reuse ssh_host rather than the
# provider-DNS `hostname` field — the daemon's copy is local-only (host_id never
# crosses the wire; the heartbeat is identified by the token), but matching keeps
# the host's logs and the dashboard fleet list naming the runner the same way.
host_id="$ssh_host"
runner_token="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/runner-token")"

# Fail loud on a stale pre-Option-B token so a wrong shape is caught here, not
# at runtime as a confusing 401 loop on the dev host.
if [[ "$runner_token" != agt_r* ]]; then
  echo "  ✗ runner-token does not start with agt_r (Option B contract)"
  echo "    got: $(printf '%s' "$runner_token" | head -c 5)... (truncated)"
  exit 1
fi

# Reject the placeholder: it satisfies the agt_r prefix but the daemon would loop
# on 401s. Mirror deploy.sh sync_env's hardened exit so the cause is named here,
# not surfaced as a confusing is-active failure after the restart below.
if [[ "$runner_token" == agt_rFAKE* ]]; then
  echo "  ✗ runner-token is the placeholder (agt_rFAKE…) — not a real token"
  echo "    mint one via the dashboard (POST /v1/runners) and update"
  echo "    op://$vault_dev/zombie-dev-worker-ant/runner-token before re-running."
  exit 1
fi

ssh_opts=(-o StrictHostKeyChecking=no -o ConnectTimeout=15 -o BatchMode=yes)
ssh_id=$(mktemp)
env_local=""
# Cover both tmpfiles: env_local holds the runner-token in cleartext between
# write and scp success, so a mid-script failure under `set -e` must not leak
# it. `${env_local:-}` is empty before mktemp runs — rm -f "" is a no-op.
trap 'rm -f "$ssh_id" "${env_local:-}"' EXIT
printf '%s\n' "$ssh_key" > "$ssh_id"
chmod 600 "$ssh_id"

echo "-- writing /opt/agentsfleet/.env on ${ssh_user}@${ssh_host}"
# Build the env body locally, then scp; avoids passing the token on the SSH
# command line where it would land in process listings on the remote host.
env_local=$(mktemp)
cat > "$env_local" <<EOF
AGENTSFLEET_API_URL=${api_url}
AGENTSFLEET_RUNNER_TOKEN=${runner_token}
RUNNER_HOST_ID=${host_id}
EOF
chmod 600 "$env_local"
scp -i "$ssh_id" "${ssh_opts[@]}" "$env_local" \
  "${ssh_user}@${ssh_host}:/opt/agentsfleet/.env"
ssh -i "$ssh_id" "${ssh_opts[@]}" "${ssh_user}@${ssh_host}" \
  "chmod 600 /opt/agentsfleet/.env"

# Sync the source env to the unit's EnvironmentFile, then restart when the runner
# binary is already installed. The unit reads
# /etc/default/agentsfleet-runner (deploy.sh's ENV_DEST) — NOT /opt/agentsfleet/.env — so a
# restart without this copy would re-read the previous /etc/default and ignore the
# vars just written. /opt/agentsfleet/.env stays the source-of-truth deploy.sh copies
# from on every CI deploy; we seed both here so either this restart or the first
# deploy exercises the new token. install -m 600 keeps it root-only.
echo "-- syncing env -> /etc/default/agentsfleet-runner + reconciling agentsfleet-runner.service"
service_action="restarted"
if ssh -i "$ssh_id" "${ssh_opts[@]}" "${ssh_user}@${ssh_host}" \
  "sudo install -m 600 -o root -g root /opt/agentsfleet/.env /etc/default/agentsfleet-runner \
   && if [ -x /usr/local/bin/agentsfleet-runner ]; then \
        sudo systemctl restart agentsfleet-runner.service; \
      else \
        sudo systemctl stop agentsfleet-runner.service && exit 42; \
      fi"; then
  :
else
  service_status=$?
  if [ "$service_status" -eq 42 ]; then
    service_action="deferred"
  else
    exit "$service_status"
  fi
fi

# A freshly bootstrapped or renamed host can have the unit before its first
# binary. Stop systemd's Restart=always loop and let deploy.sh install, start,
# and health-check the binary later in the same CI job.
if [ "$service_action" = "deferred" ]; then
  echo "  ✓ env provisioned; service start deferred until runner binary deployment"
  echo ""
  echo "✅ section 4 passed"
  exit 0
fi

# Poll up to ~10s so we don't race systemd's RestartSec=5 (a daemon that
# crashes and is in `scheduled-restart` would read as `failed` at +3s).
status=""
for _ in 1 2 3 4 5 6 7 8 9 10; do
  sleep 1
  status=$(ssh -i "$ssh_id" "${ssh_opts[@]}" "${ssh_user}@${ssh_host}" \
    "systemctl is-active agentsfleet-runner.service 2>&1 || true")
  [ "$status" = "active" ] && break
done

if [ "$status" = "active" ]; then
  echo "  ✓ agentsfleet-runner.service is active"
else
  echo "  ✗ agentsfleet-runner.service is not active (status: $status)"
  echo "    inspect: ssh ${ssh_user}@${ssh_host} journalctl -u agentsfleet-runner --no-pager -n 30"
  exit 1
fi

echo ""
echo "✅ section 4 passed"
