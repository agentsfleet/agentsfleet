#!/usr/bin/env bash
# dragonfly_cluster_test.sh — the cluster script's own tests.
#
# The parts that decide a cluster's shape are pure: which ranges a primary owns
# after a move, whether a move is legal, and the JSON pushed to every node. They
# run here against a temp slots file with no Dragonfly and no docker, so a
# regression in the split arithmetic fails `make lint-scripts` rather than the
# first slot-migration test in the integration lane, which would read as a
# datastore fault.
#
# The script is SOURCED, which its final line permits; nothing here starts a
# node. What it cannot prove — that Dragonfly accepts the document — the
# compose healthcheck and the integration lane prove on every run.

set -uo pipefail

SCRIPT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/dragonfly-cluster.sh"
readonly SCRIPT
FAILURES=0

ok()  { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); }

# A fresh data directory holding the canonical layout, with the script's
# functions loaded against it.
fixture() {
  DRAGONFLY_DATA="$(mktemp -d "${TMPDIR:-/tmp}/dragonfly-cluster-test.XXXXXX")"
  export DRAGONFLY_DATA
  # shellcheck source=scripts/dragonfly-cluster.sh
  source "$SCRIPT"
  # The script turns `-e` on for itself; a test wants to observe a failure,
  # not die of it.
  set +e
  printf '%s\n' "$CANONICAL_SLOTS" >"$SLOTS_FILE"
}

teardown() { rm -rf "$DRAGONFLY_DATA"; }

test_should_split_a_range_around_the_moved_slots() {
  local name="test_should_split_a_range_around_the_moved_slots"
  fixture
  reassign_slots 2 0 9000 9999
  local got
  got="$(sort "$SLOTS_FILE" | tr '\n' ';')"
  if [ "$got" = "0 0 8191;0 9000 9999;2 10000 16383;2 8192 8999;" ]; then
    ok "$name"
  else
    bad "$name" "layout after the move was: $got"
  fi
  teardown
}

test_should_refuse_a_move_the_source_does_not_own() {
  local name="test_should_refuse_a_move_the_source_does_not_own"
  fixture
  if owns_range 0 8000 8300; then
    bad "$name" "a range straddling two primaries was accepted"
  elif ! owns_range 0 0 8191; then
    bad "$name" "a range the primary owns whole was refused"
  else
    ok "$name"
  fi
  teardown
}

test_should_render_the_config_every_node_is_pushed() {
  local name="test_should_render_the_config_every_node_is_pushed"
  fixture
  local doc
  doc="$(render_config)"
  local want='[{"slot_ranges":[{"start":0,"end":8191}],"master":{"id":"dfly-a","ip":"127.0.0.1","port":7001},"replicas":[{"id":"dfly-a-replica","ip":"127.0.0.1","port":7002}]},{"slot_ranges":[{"start":8192,"end":16383}],"master":{"id":"dfly-b","ip":"127.0.0.1","port":7003},"replicas":[{"id":"dfly-b-replica","ip":"127.0.0.1","port":7004}]}]'
  if [ "$doc" = "$want" ]; then
    ok "$name"
  else
    bad "$name" "rendered: $doc"
  fi
  teardown
}

test_should_attach_the_migration_to_the_source_shard_only() {
  local name="test_should_attach_the_migration_to_the_source_shard_only"
  fixture
  local doc
  doc="$(render_config 2 0 8192 12287)"
  local mig='"migrations":[{"node_id":"dfly-a","ip":"127.0.0.1","port":7005,"slot_ranges":[{"start":8192,"end":12287}]}]'
  if [ "$(printf '%s' "$doc" | command grep -o '"migrations"' | wc -l | tr -d ' ')" != "1" ]; then
    bad "$name" "expected exactly one migrations entry: $doc"
  elif [[ "$doc" != *"$mig"* ]]; then
    bad "$name" "the migration must dial the target's ADMIN port with the moved range: $doc"
  elif [[ "$doc" != *'"id":"dfly-b"'*'"migrations"'* ]]; then
    bad "$name" "the migration must hang off the source shard: $doc"
  else
    ok "$name"
  fi
  teardown
}

test_should_derive_admin_ports_beside_the_data_ports() {
  local name="test_should_derive_admin_ports_beside_the_data_ports"
  fixture
  if [ "$(data_port 3)" = "7004" ] && [ "$(admin_port 0)" = "7005" ] && [ "$(admin_port 3)" = "7008" ]; then
    ok "$name"
  else
    bad "$name" "data 3=$(data_port 3) admin 0=$(admin_port 0) admin 3=$(admin_port 3)"
  fi
  teardown
}

test_should_split_a_range_around_the_moved_slots
test_should_refuse_a_move_the_source_does_not_own
test_should_render_the_config_every_node_is_pushed
test_should_attach_the_migration_to_the_source_shard_only
test_should_derive_admin_ports_beside_the_data_ports

if [ "$FAILURES" -ne 0 ]; then
  printf '%s failure(s)\n' "$FAILURES"
  exit 1
fi
