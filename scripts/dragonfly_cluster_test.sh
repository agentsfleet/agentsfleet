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
  local mig='"migrations":[{"node_id":"dfly-a","ip":"127.0.0.1","port":7008,"slot_ranges":[{"start":8192,"end":12287}]}]'
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
  if [ "$(data_port 3)" = "7004" ] && [ "$(admin_port 0)" = "7008" ] && [ "$(admin_port 3)" = "7011" ]; then
    ok "$name"
  else
    bad "$name" "data 3=$(data_port 3) admin 0=$(admin_port 0) admin 3=$(admin_port 3)"
  fi
  teardown
}

# The TLS node's data port follows the cluster's four, inside the published
# range, and its admin port follows theirs — nothing overlaps.
test_should_place_the_tls_node_after_the_cluster_ports() {
  local name="test_should_place_the_tls_node_after_the_cluster_ports"
  fixture
  if [ "$(data_port "$TLS_NODE")" = "7005" ] && [ "$(admin_port "$TLS_NODE")" = "7012" ]; then
    ok "$name"
  else
    bad "$name" "tls data=$(data_port "$TLS_NODE") tls admin=$(admin_port "$TLS_NODE")"
  fi
  teardown
}

# The source cluster's two nodes follow the TLS node, and the admin block
# starts past EVERY data port. That is the invariant the offset encodes: a node
# added without widening it binds its data port onto node 0's admin port, and
# the failure is a bind error inside a container nobody reads.
test_should_place_the_source_cluster_after_every_other_data_port() {
  local name="test_should_place_the_source_cluster_after_every_other_data_port"
  fixture
  if [ "$(data_port "${SOURCE_NODES[0]}")" = "7006" ] &&
     [ "$(data_port "${SOURCE_NODES[1]}")" = "7007" ] &&
     [ "$(data_port "${SOURCE_NODES[1]}")" = "$(data_port "$LAST_NODE")" ] &&
     [ "$(admin_port 0)" -gt "$(data_port "$LAST_NODE")" ]; then
    ok "$name"
  else
    bad "$name" "src=$(data_port "${SOURCE_NODES[0]}")-$(data_port "${SOURCE_NODES[1]}") last=$(data_port "$LAST_NODE") admin0=$(admin_port 0)"
  fi
  teardown
}

# The source is its own cluster: two primaries, no replicas, the whole slot
# space between them. A gap leaves keys nothing owns; an overlap makes an
# inventory count them twice.
test_should_render_a_source_config_covering_every_slot_once() {
  local name="test_should_render_a_source_config_covering_every_slot_once"
  fixture
  local rendered expected
  rendered="$(render_source_config)"
  expected='[{"slot_ranges":[{"start":0,"end":8191}],"master":{"id":"dfly-src-a","ip":"127.0.0.1","port":7006},"replicas":[]},{"slot_ranges":[{"start":8192,"end":16383}],"master":{"id":"dfly-src-b","ip":"127.0.0.1","port":7007},"replicas":[]}]'
  if [ "$rendered" = "$expected" ]; then
    ok "$name"
  else
    bad "$name" "$rendered"
  fi
  teardown
}

# Neither cluster names a node of the other. That separation is what makes the
# rehearsal a switch between deployments rather than a copy within one.
test_should_keep_the_source_and_target_clusters_disjoint() {
  local name="test_should_keep_the_source_and_target_clusters_disjoint"
  fixture
  local overlap=0 src tgt
  for src in "${SOURCE_IDS[@]}"; do
    for tgt in "${NODE_IDS[@]}"; do
      [ "$src" = "$tgt" ] && overlap=1
    done
  done
  for src in "${SOURCE_NODES[@]}"; do
    [ "$src" -le "$TLS_NODE" ] && overlap=1
  done
  if [ "$overlap" -eq 0 ]; then
    ok "$name"
  else
    bad "$name" "source ids or indices collide with the target cluster"
  fi
  teardown
}

test_should_split_a_range_around_the_moved_slots
test_should_refuse_a_move_the_source_does_not_own
test_should_render_the_config_every_node_is_pushed
test_should_attach_the_migration_to_the_source_shard_only
test_should_derive_admin_ports_beside_the_data_ports
test_should_place_the_tls_node_after_the_cluster_ports
test_should_place_the_source_cluster_after_every_other_data_port
test_should_render_a_source_config_covering_every_slot_once
test_should_keep_the_source_and_target_clusters_disjoint

if [ "$FAILURES" -ne 0 ]; then
  printf '%s failure(s)\n' "$FAILURES"
  exit 1
fi
