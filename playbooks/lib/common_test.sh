#!/usr/bin/env bash
# Regression tests for the SSH failure-diagnosis helpers in common.sh.
#
#     bash playbooks/lib/common_test.sh
#
# These guard the Jul 28, 2026 dev-worker outage: enabling Tailscale SSH on the
# worker moved the access decision from sshd to the tailnet policy, and CI died
# with a bare exit 255 that named neither the layer nor the missing rule.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_DIR/common.sh"
# common.sh enables -e for the scripts that source it; the assertions below
# deliberately run failing commands and must survive them.
set +e

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

assert_contains() {
  local name="$1" haystack="$2" needle="$3"
  case "$haystack" in
    *"$needle"*) return 0 ;;
    *)
      bad "$name" "expected to contain '${needle}', got: ${haystack}"
      return 1
      ;;
  esac
}

readonly TAILNET_DENIAL='tailscale: tailnet policy does not permit you to SSH to this node'

test_should_name_the_missing_tag_rule_on_a_policy_denial() {
  local name="test_should_name_the_missing_tag_rule_on_a_policy_denial"
  local out
  out="$(playbooks_explain_ssh_failure "$TAILNET_DENIAL" 2>&1)"
  assert_contains "$name" "$out" 'tag:ci' || return
  assert_contains "$name" "$out" 'tag:worker' || return
  assert_contains "$name" "$out" 'tailnet-policy.hujson' || return
  ok "$name"
}

test_should_point_at_the_ssh_flag_when_host_keys_are_absent() {
  local name="test_should_point_at_the_ssh_flag_when_host_keys_are_absent"
  local out
  out="$(playbooks_explain_ssh_failure 'Host key verification failed.' 2>&1)"
  assert_contains "$name" "$out" '--ssh' || return
  ok "$name"
}

test_should_stay_silent_on_an_unrecognised_failure() {
  local name="test_should_stay_silent_on_an_unrecognised_failure"
  local out
  out="$(playbooks_explain_ssh_failure 'some unrelated transport error' 2>&1)"
  if [ -n "$out" ]; then
    bad "$name" "expected no diagnosis, got: ${out}"
    return
  fi
  ok "$name"
}

test_should_pass_stdout_through_when_the_command_succeeds() {
  local name="test_should_pass_stdout_through_when_the_command_succeeds"
  local out status
  out="$(playbooks_ssh_run 'probe' bash -c 'echo remote-ok')"
  status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "expected exit 0, got ${status}"
    return
  fi
  assert_contains "$name" "$out" 'remote-ok' || return
  ok "$name"
}

test_should_preserve_exit_status_and_explain_a_policy_denial() {
  local name="test_should_preserve_exit_status_and_explain_a_policy_denial"
  local out status
  out="$(playbooks_ssh_run 'provision env' \
    bash -c "printf '%s\n' '${TAILNET_DENIAL}' >&2; exit 255" 2>&1)"
  status=$?
  if [ "$status" -ne 255 ]; then
    bad "$name" "expected exit 255 to survive the wrapper, got ${status}"
    return
  fi
  assert_contains "$name" "$out" 'provision env' || return
  assert_contains "$name" "$out" 'tag:worker' || return
  ok "$name"
}

test_should_name_the_missing_tag_rule_on_a_policy_denial
test_should_point_at_the_ssh_flag_when_host_keys_are_absent
test_should_stay_silent_on_an_unrecognised_failure
test_should_pass_stdout_through_when_the_command_succeeds
test_should_preserve_exit_status_and_explain_a_policy_denial

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
