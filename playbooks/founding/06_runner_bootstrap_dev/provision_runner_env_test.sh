#!/usr/bin/env bash
# Regression tests for provisioning runner env before and after binary install.
#
#     bash playbooks/founding/06_runner_bootstrap_dev/provision_runner_env_test.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_UNDER_TEST="$SCRIPT_DIR/04_provision_runner_env.sh"

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
readonly STUB_DIR="$WORK_DIR/bin"
readonly SSH_CALLS="$WORK_DIR/ssh-calls.log"
mkdir -p "$STUB_DIR"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

cat >"$STUB_DIR/op" <<'STUB'
#!/usr/bin/env bash
case "${2:-}" in
  */ssh-private-key)   printf 'stub-private-key\n' ;;
  */tailscale-hostname) printf 'stub-host\n' ;;
  */deploy-user)       printf 'stub-user\n' ;;
  */runner-token)      printf 'agt_rREAL_TEST_TOKEN\n' ;;
  *) exit 1 ;;
esac
STUB

cat >"$STUB_DIR/scp" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB

cat >"$STUB_DIR/ssh" <<'STUB'
#!/usr/bin/env bash
command="${*: -1}"

sudo() {
  case "${1:-}" in
    install)
      printf 'env-sync\n' >>"$SSH_CALLS"
      ;;
    systemctl)
      printf '%s\n' "${2:-}" >>"$SSH_CALLS"
      ;;
    *)
      printf 'unexpected sudo command: %s\n' "$*" >&2
      return 1
      ;;
  esac
}

case "$command" in
  "chmod 600 /opt/agentsfleet/.env") exit 0 ;;
  *"sudo install -m 600"*)
    printf '%s' "${REMOTE_STARTUP_OUTPUT:-}"
    command="${command/'[ -x /usr/local/bin/agentsfleet-runner ]'/'[ "'"${REMOTE_BINARY_PRESENT:-0}"'" = 1 ]'}"
    eval "$command"
    ;;
  *"systemctl is-active"*)
    printf 'status-check\n' >>"$SSH_CALLS"
    printf '%s\n' "${REMOTE_SERVICE_STATUS:-active}"
    ;;
  *)
    printf 'unexpected: %s\n' "$command" >&2
    exit 1
    ;;
esac
STUB

chmod +x "$STUB_DIR/op" "$STUB_DIR/scp" "$STUB_DIR/ssh"

run_provision() {
  : >"$SSH_CALLS"
  env PATH="$STUB_DIR:$PATH" \
    SSH_CALLS="$SSH_CALLS" \
    OP_READ_MIN_INTERVAL_SECONDS=0 \
    "$@" \
    bash "$SCRIPT_UNDER_TEST" 2>&1
}

test_should_defer_health_check_when_runner_binary_is_absent() {
  local name="test_should_defer_health_check_when_runner_binary_is_absent"
  local output status=0
  output="$(run_provision \
    REMOTE_BINARY_PRESENT=0 \
    REMOTE_STARTUP_OUTPUT=$'remote startup output\n')" || status=$?

  if [[ "$status" -ne 0 ]]; then
    bad "$name" "provisioning failed on a host awaiting its first binary: $output"
  elif ! grep -q '^stop$' "$SSH_CALLS" || grep -q '^restart$' "$SSH_CALLS"; then
    bad "$name" "missing binary did not stop the service retry loop"
  elif grep -q '^status-check$' "$SSH_CALLS"; then
    bad "$name" "provisioning checked service health before a runner binary existed"
  elif [[ "$output" != *"service start deferred until runner binary deployment"* ]]; then
    bad "$name" "provisioning did not explain the deferred service start: $output"
  else
    ok "$name"
  fi
}

test_should_restart_and_verify_when_runner_binary_exists() {
  local name="test_should_restart_and_verify_when_runner_binary_exists"
  local output status=0
  output="$(run_provision REMOTE_BINARY_PRESENT=1 REMOTE_SERVICE_STATUS=active)" || status=$?

  if [[ "$status" -ne 0 ]]; then
    bad "$name" "provisioning failed with an installed, healthy runner: $output"
  elif ! grep -q '^restart$' "$SSH_CALLS" || grep -q '^stop$' "$SSH_CALLS" || \
      ! grep -q '^status-check$' "$SSH_CALLS"; then
    bad "$name" "installed runner was not restarted and health-checked"
  elif [[ "$output" != *"agentsfleet-runner.service is active"* ]]; then
    bad "$name" "provisioning omitted the active-service result: $output"
  else
    ok "$name"
  fi
}

test_should_defer_health_check_when_runner_binary_is_absent
test_should_restart_and_verify_when_runner_binary_exists

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
