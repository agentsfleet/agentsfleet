#!/usr/bin/env bash
# run-zig-shards.sh — run one Zig test binary as N concurrent shard processes.
#
#     run-zig-shards.sh <shard-count> <binary> [prefix...]
#
# `prefix` wraps the binary, so a leak gate is expressed by passing the gate as
# the prefix:
#
#     run-zig-shards.sh 4 zig-out/bin/agentsfleetd-tests valgrind --leak-check=full
#
# Exit 0 only when every shard exits 0. The binary must be built with
# src/build/test_runner_shard.zig, which reads the two variables exported below;
# a binary on the default runner ignores them and every shard runs the whole
# suite, which is slow but never wrong.
#
# Output is captured per shard and replayed after the wait, FAILING SHARDS
# FIRST. Interleaving N processes onto one stream makes a Valgrind report or a
# Zig error-return trace unreadable, and the failing shard is the only one
# anybody wants to read.
set -euo pipefail

if (( $# < 2 )); then
  echo "usage: run-zig-shards.sh <shard-count> <binary> [prefix...]" >&2
  exit 2
fi

shard_count=$1
binary=$2
shift 2
prefix=("$@")

case "$shard_count" in
  ''|*[!0-9]*|0) echo "✗ shard-count must be a positive integer, got '$shard_count'" >&2; exit 2 ;;
esac

if [[ ! -x "$binary" ]]; then
  echo "✗ not an executable test binary: $binary" >&2
  exit 2
fi

# Name the variables once, here; src/build/test_runner_shard.zig spells the same
# two identifiers. They are exported per child rather than globally so a shard's
# index cannot leak into a sibling.
readonly SHARD_INDEX_ENV="AGENTSFLEET_TEST_SHARD_INDEX"
readonly SHARD_COUNT_ENV="AGENTSFLEET_TEST_SHARD_COUNT"

log_dir=$(mktemp -d)
trap 'rm -rf "$log_dir"' EXIT

pids=()
for (( index = 0; index < shard_count; index++ )); do
  env "$SHARD_INDEX_ENV=$index" "$SHARD_COUNT_ENV=$shard_count" \
    "${prefix[@]}" "$binary" > "$log_dir/$index.log" 2>&1 &
  pids+=("$!")
done

statuses=()
overall=0
for index in "${!pids[@]}"; do
  if wait "${pids[index]}"; then
    statuses[index]=0
  else
    statuses[index]=$?
    overall=1
  fi
done

for index in "${!statuses[@]}"; do
  if (( statuses[index] != 0 )); then
    echo "── shard $index/$shard_count FAILED (exit ${statuses[index]}) ──"
    cat "$log_dir/$index.log"
  fi
done
for index in "${!statuses[@]}"; do
  if (( statuses[index] == 0 )); then
    echo "── shard $index/$shard_count ok ──"
    cat "$log_dir/$index.log"
  fi
done

if (( overall != 0 )); then
  echo "✗ $(printf '%s\n' "${statuses[@]}" | grep -cv '^0$') of $shard_count shards failed"
  exit 1
fi
echo "✓ all $shard_count shards passed"
