#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
PLAYBOOK="$SCRIPT_DIR/001_playbook.md"
VERIFY="$SCRIPT_DIR/03_verify.sh"
QUEUE_CONSTANTS="$REPO_ROOT/src/agentsfleetd/queue/constants.zig"
OUTBOUND="$REPO_ROOT/src/agentsfleetd/queue/connector_outbound.zig"
ACTIVITY="$REPO_ROOT/src/agentsfleetd/events/activity_channel.zig"

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

test_source_and_playbook_share_topology() {
  require_literal source_fleet_stream "$QUEUE_CONSTANTS" \
    'pub const fleet_stream_prefix = "fleet:";'
  require_literal source_ready_index "$QUEUE_CONSTANTS" \
    'pub const ready_index_key = "fleet:ready";'
  require_literal source_fleet_group "$QUEUE_CONSTANTS" \
    'pub const fleet_consumer_group = "fleet_lease";'
  require_literal source_outbound_stream "$OUTBOUND" \
    'pub const STREAM_KEY = "connector:outbound";'
  require_literal source_activity_channel "$ACTIVITY" \
    'pub const SUFFIX = ":activity";'

  for literal in \
    'fleet:{fleet_id}:events' \
    'fleet_lease' \
    'fleet:ready' \
    'connector:outbound' \
    'fleet:{fleet_id}:activity'; do
    require_literal "playbook_${literal//[^a-zA-Z0-9]/_}" \
      "$PLAYBOOK" "$literal"
  done
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

test_source_and_playbook_share_topology
test_retired_agent_topology_is_absent
test_restart_is_required

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
