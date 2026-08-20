#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"
# shellcheck source=../egress_host_deps.sh
source "$SCRIPT_DIR/../egress_host_deps.sh"

# Readiness probe budget: ~60s of tunnel reconnect, then fail. Overridable so
# the suite can exercise the retry without sleeping through it.
READYZ_ATTEMPTS="${RUNNER_READYZ_ATTEMPTS:-6}"
READYZ_RETRY_SECONDS="${RUNNER_READYZ_RETRY_SECONDS:-10}"
readonly READYZ_ATTEMPTS READYZ_RETRY_SECONDS
readonly READYZ_TIMEOUT_SECONDS=10

verify_files_and_service() {
  runner_remote "
    set -e
    test \"\$(stat -c %a /opt/agentsfleet/.env)\" = 600
    test -x /opt/agentsfleet/deploy/deploy.sh
    test -x /usr/local/bin/agentsfleet-runner
    test -f /etc/systemd/system/agentsfleet-runner.service
    systemctl is-enabled --quiet agentsfleet-runner.service
    test \"\$(systemctl is-active agentsfleet-runner.service)\" = active
    test \"\$(systemctl show agentsfleet-runner.service --property=Delegate --value)\" = yes
    test \"\$(systemctl show agentsfleet-runner.service --property=DelegateSubgroup --value)\" = runner
  "
}

verify_cgroup_controllers() {
  local report service_subtree
  report="$(runner_remote '
    set -e
    cgroup_path="$(systemctl show agentsfleet-runner.service --property=ControlGroup --value)"
    test "$cgroup_path" = /system.slice/agentsfleet-runner.service
    root_path='"$CGROUP_ROOT"'
    slice_path="$root_path/system.slice"
    service_path="$root_path$cgroup_path"
    for entry in \
      "root:$root_path" \
      "slice:$slice_path" \
      "service:$service_path"
    do
      label="${entry%%:*}"
      path="${entry#*:}"
      printf "%s_controllers=" "$label"
      sudo cat "$path/cgroup.controllers"
      printf "%s_subtree=" "$label"
      sudo cat "$path/cgroup.subtree_control"
    done
  ')"
  service_subtree="$(
    printf '%s\n' "$report" |
      sed -n 's/^service_subtree=//p'
  )"

  local controller
  for controller in $REQUIRED_CGROUP_CONTROLLERS; do
    case " $service_subtree " in
      *" $controller "*) ;;
      *)
        echo "ERROR: cgroup controller is not delegated: $controller" >&2
        printf '%s\n' "$report" >&2
        return 1
        ;;
    esac
  done
}

# The runner reaches the control plane through Cloudflare, so this probe reads
# the edge as well as the box. A cloudflared connector re-registers for a few
# seconds after every API deploy, and Cloudflare answers 530 (error 1033 — no
# connector for the hostname) for that whole window. The development lane ships
# the API and the runner in the same run, so a single-shot curl lands inside it
# and fails a runner that is fine: run 32385110520 died that way one second
# after the deploy reported the service active, with /readyz green moments
# later. Retry over a bounded window and name the last status, so a genuine
# outage still fails loud — and says what the edge answered.
verify_control_plane() {
  local attempt=1 status=""
  while [ "$attempt" -le "$READYZ_ATTEMPTS" ]; do
    status="$(
      runner_remote "curl -sS -o /dev/null -m $READYZ_TIMEOUT_SECONDS -w '%{http_code}' '$RUNNER_API_URL/readyz' || true"
    )"
    if [ "$status" = 200 ]; then
      echo "  ✓ control plane: $RUNNER_API_URL/readyz 200 (attempt $attempt/$READYZ_ATTEMPTS)"
      return 0
    fi
    echo "  … control plane: $RUNNER_API_URL/readyz answered ${status:-000} (attempt $attempt/$READYZ_ATTEMPTS)"
    attempt=$((attempt + 1))
    if [ "$attempt" -le "$READYZ_ATTEMPTS" ]; then
      sleep "$READYZ_RETRY_SECONDS"
    fi
  done
  echo "ERROR: control plane unreachable from $RUNNER_ITEM after $READYZ_ATTEMPTS attempts: $RUNNER_API_URL/readyz answered ${status:-000}" >&2
  return 1
}

verify_runner_identity() {
  runner_remote "sudo sh -c 'set -a; . /etc/default/agentsfleet-runner; set +a; exec /usr/local/bin/agentsfleet-runner doctor >/dev/null'"
}

main() {
  runner_load_target
  echo "Verifying $RUNNER_ITEM in ${ENV} via Tailscale SSH"

  runner_remote "test \"\$(tailscale status --json | jq -r .Self.Online)\" = true"
  egress_probe_remote runner_remote
  verify_files_and_service
  verify_cgroup_controllers
  verify_control_plane
  verify_runner_identity

  echo "PASS: $RUNNER_ITEM is ready"
}

main "$@"
