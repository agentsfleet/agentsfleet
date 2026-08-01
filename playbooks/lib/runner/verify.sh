#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"
# shellcheck source=../egress_host_deps.sh
source "$SCRIPT_DIR/../egress_host_deps.sh"

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
    root_path=/sys/fs/cgroup
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
  for controller in cpu memory pids; do
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

verify_control_plane() {
  runner_remote "curl -fsS '$RUNNER_API_URL/readyz' >/dev/null"
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
