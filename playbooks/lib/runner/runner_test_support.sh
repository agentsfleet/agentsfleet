#!/usr/bin/env bash
# Stub harness shared by the runner playbook suites.
#
#     source "$(dirname "${BASH_SOURCE[0]}")/runner_test_support.sh"
#
# Split out of runner_test.sh when the verify-lane cases pushed that file past
# the 350-line cap. The `op` and `tailscale` stubs plus the hermetic
# `run_script` launcher are the entire world those suites execute in, and both
# runner_test.sh and runner_verify_test.sh need them identically — a second
# copy would drift the moment one suite taught its stub a new answer.

set -uo pipefail

RUNNER_TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../test_search.sh
source "$RUNNER_TEST_DIR/../test_search.sh"
readonly PREPARE="$RUNNER_TEST_DIR/prepare.sh"
readonly DEPLOY="$RUNNER_TEST_DIR/deploy.sh"
readonly VERIFY="$RUNNER_TEST_DIR/verify.sh"
readonly DEPLOY_SCRIPT="$RUNNER_TEST_DIR/../../../deploy/baremetal/deploy.sh"
readonly REPO_ROOT="$RUNNER_TEST_DIR/../../.."
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
readyz_counter="$work_dir/readyz-attempts"
mkdir -p "$stub_dir"
trap 'rm -rf "$work_dir"' EXIT

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

report_results() {
  printf '\n%d passed, %d failed\n' "$passed" "$failed"
  [ "$failed" -eq 0 ]
}

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read)
    case "${2:-}" in
      */tailscale-hostname) printf 'runner-host\n' ;;
      */deploy-user) printf 'runner-user\n' ;;
      */runner-token) printf '%s\n' "${STUB_TOKEN:-agt_rREAL_TOKEN}" ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/tailscale" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
if [ "${1:-}" = status ]; then
  printf '{"Self":{"Online":true}}\n'
  exit 0
fi
command="${*: -1}"
if [ -n "${TAILSCALE_FAIL_MATCH:-}" ] && [[ "$command" == *"$TAILSCALE_FAIL_MATCH"* ]]; then
  exit 9
fi
if [ "${STUB_EXECUTE_CGROUP_GATE:-0}" = 1 ] &&
  [[ "$command" == *"/sys/fs/cgroup/cgroup.controllers"* ]]
then
  command="${command//\/sys\/fs\/cgroup/$STUB_CGROUP_ROOT}"
  bash -c "$command"
  exit $?
fi
case "$command" in
  *"BWRAP_VERSION="*)
    printf '%s\n' \
      'BWRAP_VERSION=bwrap 0.11.0' \
      'BWRAP_INFO_FD=1' \
      'BWRAP_BLOCK_FD=1' \
      'NFT_VERSION=nftables v1.0' \
      'IP_VERSION=iproute2-6.0'
    ;;
  *"cgroup.subtree_control"*)
    printf '%s\n' \
      'root_controllers=cpu memory pids' \
      'root_subtree=cpu memory pids' \
      'slice_controllers=cpu memory pids' \
      'slice_subtree=cpu memory pids' \
      'service_controllers=cpu memory pids' \
      "service_subtree=${STUB_CGROUP_CONTROLLERS:-cpu memory pids}"
    ;;
  *"/readyz"*)
    # One status per probe, last entry repeating, so a case can hand the
    # readiness loop a Cloudflare 530 that clears (tunnel reconnect) or one
    # that never does (real outage).
    # shellcheck disable=SC2206  # deliberate split of the space-separated list
    statuses=(${STUB_READYZ_STATUSES:-200})
    index=0
    [ -f "$READYZ_COUNTER" ] && index="$(cat "$READYZ_COUNTER")"
    printf '%s\n' "$((index + 1))" >"$READYZ_COUNTER"
    [ "$index" -ge "${#statuses[@]}" ] && index=$(("${#statuses[@]}" - 1))
    printf '%s\n' "${statuses[index]}"
    ;;
esac
cat >/dev/null || true
STUB
chmod +x "$stub_dir/op" "$stub_dir/tailscale"
runner_binary="$work_dir/agentsfleet-runner"
printf 'runner\n' >"$runner_binary"
cgroup_fixture="$work_dir/cgroup"
mkdir -p "$cgroup_fixture"

run_script() {
  : >"$calls"
  rm -f "$readyz_counter"
  # The test must supply its whole world, so the child sees ONLY what this
  # harness assigns plus the VAR=val pairs each case passes. Cases that mean to
  # exercise a variable pass it as an argument below, which still wins.
  #
  # The repository actively encourages a polluted environment:
  # `.githooks/post-checkout` links `.env.runner.local`, which carries
  # `AGENTSFLEET_API_URL`, and `common.sh` reads it as
  # `${AGENTSFLEET_API_URL:-$expected_api_url}` then refuses the deploy when it
  # disagrees with ENV's endpoint. An ambient dev endpoint therefore failed the
  # ENV=prod case on a workstation while Continuous Integration — with a bare
  # environment — passed.
  #
  # `env -i` rather than `env -u` per known name: the enumerated form fixes the
  # variables someone remembered, and this file gains readers faster than it
  # gains maintainers of that list. Wiping closes the whole class, including
  # the next input the runner library learns to read. PATH is composed before
  # the wipe, so the stub dir still shadows the real tools while the system
  # tail keeps bash findable.
  env -i \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    READYZ_COUNTER="$readyz_counter" \
    RUNNER_BINARY="$runner_binary" \
    RUNNER_VERSION=test-build \
    ALLOW_VAULT_READS=1 \
    "$@" 2>&1
}
