#!/usr/bin/env bash
#
# Guards the two things in the Redis teardown that a human acts on.
#
# It deliberately does NOT pin Redis key names against the Zig constants. The
# teardown is `FLUSHALL` (02_teardown.sh) and the verification is `DBSIZE == 0`
# (03_verify.sh) — both name-blind, so no key name is load-bearing for either
# step. Ten assertions used to pin `fleet:ready`, `connector:outbound` and the
# rest against queue/constants.zig; they proved only that descriptive prose
# matched a constant, went red on a reformat, and taxed every rename without
# preventing any failure. Do not add them back.
#
# What remains is prose an operator EXECUTES, which is why pinning it is a real
# test: the restart instruction (a flush destroys the `fleet_lease` consumer
# group while running agentsfleetd processes still hold their cursors), and the
# absence of the retired `agent` noun.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLAYBOOK="$SCRIPT_DIR/001_playbook.md"
VERIFY="$SCRIPT_DIR/03_verify.sh"

passed=0
failed=0

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

require_literal() {
  local name="$1"
  local file="$2"
  local literal="$3"
  if grep -Fq "$literal" "$file"; then
    ok "$name"
  else
    bad "$name" "$file does not contain: $literal"
  fi
}

test_retired_agent_topology_is_absent() {
  local name="retired_agent_topology_is_absent"
  if grep -Eq 'agent:\{agent_id\}:events|agent_lease|/agents' \
    "$PLAYBOOK" "$VERIFY"; then
    bad "$name" "Redis teardown still names the retired agent topology"
  else
    ok "$name"
  fi
}

test_restart_is_required() {
  require_literal restart_after_flush "$PLAYBOOK" \
    'Restart or redeploy every `agentsfleetd` machine'
  require_literal verify_prints_restart "$VERIFY" \
    'restart or redeploy every agentsfleetd machine'
}

test_retired_agent_topology_is_absent
test_restart_is_required

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
