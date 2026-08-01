#!/usr/bin/env bash
# M4_001 Section 3: deploy readiness gate
# Verifies playbook step 6.0:
#   - /opt/agentsfleet/deploy/deploy.sh exists and is executable
#   - /opt/agentsfleet/deploy/agentsfleet-runner.service exists
#     (the M80 cutover folded both units into the single
#      agentsfleet-runner daemon — see runner_fleet.md)
#   - /opt/agentsfleet/.env exists with correct permissions (600)
#   - Systemd units installed in /etc/systemd/system/
set -euo pipefail

echo ""
echo "== M4_001 Section 3: deploy readiness =="

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
missing=0

# Single host-resident unit since the M80 cutover folded the prior units into it.
readonly RUNNER_UNIT="agentsfleet-runner.service"
readonly REQUIRE_RUNNER_CGROUP_DELEGATION="${REQUIRE_RUNNER_CGROUP_DELEGATION:-0}"
# Pre-deploy host probe. Independent of the post-deploy delegation check above:
# this one reads facts that hold with no runner running, so it can gate the
# deploy itself rather than only report after a runner is already placed.
readonly REQUIRE_HOST_CGROUP_CAPABILITY="${REQUIRE_HOST_CGROUP_CAPABILITY:-0}"
readonly CGROUP_MOUNT="/sys/fs/cgroup"
# Same three the runner enables in its delegated subtree at startup
# (src/runner/engine/CgroupScope.zig). The two lists must stay identical — a
# controller required here but not enabled there fails every deploy.
readonly REQUIRED_CGROUP_CONTROLLERS="cpu memory pids"
readonly RUNNER_CGROUP_SUBGROUP="runner"
readonly CGROUP_PARENT_SLICE="system.slice"

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

# SSH connection details from vault.
# Vault hostname field should be the Tailscale hostname (CI joins tailnet before this step).
ssh_key="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/ssh-private-key" || true)"
ssh_host="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/tailscale-hostname" || true)"
ssh_user="$(op_read_with_retry "op://$vault_dev/zombie-dev-worker-ant/deploy-user" || true)"

if [ -z "$ssh_key" ] || [ -z "$ssh_host" ] || [ -z "$ssh_user" ]; then
  echo "  ✗ Cannot establish SSH — missing vault refs or env vars. Run section 1 first."
  exit 1
fi

# Write key to temp file (process substitution may not work in all CI shells)
_ssh_key_file=$(mktemp)
printf '%s\n' "$ssh_key" > "$_ssh_key_file"
chmod 600 "$_ssh_key_file"
trap 'rm -f "$_ssh_key_file"' EXIT

remote_cmd() {
  ssh -i "$_ssh_key_file" \
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

check_runner_cgroup_delegation() {
  local delegated
  delegated="$(remote_cmd "systemctl show '$RUNNER_UNIT' --property=Delegate --value")"
  if [ "$delegated" != "yes" ]; then
    echo "  ✗ runner cgroup delegation: Delegate=$delegated (expected yes)"
    missing=$((missing + 1))
    return
  fi

  local subgroup
  subgroup="$(remote_cmd "systemctl show '$RUNNER_UNIT' --property=DelegateSubgroup --value")"
  if [ "$subgroup" != "$RUNNER_CGROUP_SUBGROUP" ]; then
    echo "  ✗ runner cgroup delegation: DelegateSubgroup=$subgroup (expected $RUNNER_CGROUP_SUBGROUP)"
    missing=$((missing + 1))
    return
  fi

  local cgroup_path
  cgroup_path="$(remote_cmd "systemctl show '$RUNNER_UNIT' --property=ControlGroup --value")"
  case "$cgroup_path" in
    /system.slice/agentsfleet-runner.service) ;;
    *)
      echo "  ✗ runner cgroup delegation: unexpected control group '$cgroup_path'"
      missing=$((missing + 1))
      return
      ;;
  esac

  local enabled_controllers absent controller
  enabled_controllers="$(remote_cmd "sudo -n cat '$CGROUP_MOUNT$cgroup_path/cgroup.subtree_control'")"
  absent="$(absent_controllers "$enabled_controllers")"
  if [ -n "$absent" ]; then
    for controller in $absent; do
      echo "  ✗ runner cgroup delegation: controller '$controller' is not enabled"
      missing=$((missing + 1))
    done
    echo "    enabled: '$enabled_controllers' — the daemon writes these at startup;"
    echo "    check journalctl -u agentsfleet-runner for cgroup_controllers_unavailable"
    return
  fi

  echo "  ✓ runner cgroup delegation: $cgroup_path ($enabled_controllers)"
}

# Every required controller absent from `enabled`, space-separated, or empty
# when all are present. Reporting the full set matters: a host missing all three
# used to report only `cpu`, so an operator fixed one, redeployed, and met the
# next — three deploy cycles to learn one fact.
absent_controllers() {
  local enabled=" $1 "
  local controller absent=""
  for controller in $REQUIRED_CGROUP_CONTROLLERS; do
    case "$enabled" in
      *" $controller "*) ;;
      *) absent="${absent}${absent:+ }$controller" ;;
    esac
  done
  printf '%s' "$absent"
}

# Pre-deploy gate: can this HOST enforce limits at all? Reads the kernel's root
# controller set and the parent slice's delegation — both true with no runner
# running, which is what lets this gate the deploy instead of trailing it. A
# kernel booted into cgroup v1 (or without CONFIG_CGROUP_SCHED) fails the first
# check; a systemd that does not delegate down to the slice fails the second.
check_host_cgroup_capability() {
  local root_controllers slice_controllers absent controller

  root_controllers="$(remote_cmd "cat '$CGROUP_MOUNT/cgroup.controllers'")"
  absent="$(absent_controllers "$root_controllers")"
  if [ -n "$absent" ]; then
    for controller in $absent; do
      echo "  ✗ host cgroup capability: kernel offers no '$controller' controller"
      missing=$((missing + 1))
    done
    echo "    root cgroup.controllers: '$root_controllers' (cgroup v2 unified required)"
    return
  fi

  slice_controllers="$(remote_cmd "cat '$CGROUP_MOUNT/$CGROUP_PARENT_SLICE/cgroup.subtree_control'")"
  absent="$(absent_controllers "$slice_controllers")"
  if [ -n "$absent" ]; then
    for controller in $absent; do
      echo "  ✗ host cgroup capability: $CGROUP_PARENT_SLICE does not delegate '$controller'"
      missing=$((missing + 1))
    done
    return
  fi

  echo "  ✓ host cgroup capability: $CGROUP_PARENT_SLICE delegates ($slice_controllers)"
}

# 6.1 Deploy artifacts
echo "-- checking deploy artifacts (step 6.1)"
check_remote_file "/opt/agentsfleet/deploy/deploy.sh" "deploy script"
check_remote_executable "/opt/agentsfleet/deploy/deploy.sh" "deploy script"
check_remote_file "/opt/agentsfleet/deploy/$RUNNER_UNIT" "runner unit"

# 6.2 Environment file
echo "-- checking .env (step 6.2)"
check_remote_file "/opt/agentsfleet/.env" "env file" "600"

# 6.3 Systemd units installed
echo "-- checking systemd units (step 6.3)"
check_remote_file "/etc/systemd/system/$RUNNER_UNIT" "systemd runner unit"

if [ "$REQUIRE_HOST_CGROUP_CAPABILITY" = "1" ]; then
  echo "-- checking host cgroup capability (pre-deploy)"
  check_host_cgroup_capability
fi

if [ "$REQUIRE_RUNNER_CGROUP_DELEGATION" = "1" ]; then
  echo "-- checking delegated runner cgroup"
  check_runner_cgroup_delegation
fi

# 6.4 Egress host dependencies — records bwrap/nftables/iproute2
# versions in the CI log and fails loud if a box cannot enforce per-lease egress.
echo "-- checking egress host deps (step 6.4)"
# shellcheck source=../../lib/egress_host_deps.sh
. "$(dirname "${BASH_SOURCE[0]}")/../../lib/egress_host_deps.sh"
egress_probe_remote remote_cmd || missing=$((missing + 1))

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 3 failed: $missing issue(s) detected"
  exit 1
fi

echo ""
echo "✅ section 3 passed"
