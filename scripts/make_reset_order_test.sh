#!/usr/bin/env bash
# make_reset_order_test.sh — the reset/migrate ordering and the bench guard,
# read from make's own dependency graph.
#
# `make -j` runs sibling prerequisites at the same time, so the only ordering
# it honours is an edge in the graph. These cases read that graph
# (`make -pq`, which prints the rule database and runs no recipe) rather than
# a dry run's output, whose order is the serial one and proves nothing about
# a parallel run. Nothing here touches Docker, a database or the network.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT
readonly MIGRATE="_migrate-test-db"
readonly RESET="_reset-test-db"
readonly INFRA="_ensure-test-infra"
readonly BENCH_LANES=(bench-lease bench-steer bench-outbound bench-cardinality bench-tail)
FAILURES=0

ok()  { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); }

# The prerequisites make records for target $1, with any further arguments
# passed to make as variable assignments. Empty when the target has none.
prerequisites() {
  local target=$1
  shift
  make --no-print-directory -C "$ROOT" -pq "$target" "$@" 2>/dev/null \
    | awk -v target="$target" 'index($0, target ":") == 1 { sub(/^[^:]*:[[:space:]]*/, ""); print; exit }'
}

# Whether word $2 is one of the words in $1.
has_word() {
  case " $1 " in
    *" $2 "*) return 0 ;;
    *) return 1 ;;
  esac
}

test_should_migrate_only_after_the_reset() {
  local name="test_should_migrate_only_after_the_reset" found
  # Cleared on make's command line, which outranks a caller's exported
  # KEEP_TEST_STATE=1: the case asks about the default graph.
  found="$(prerequisites "$MIGRATE" KEEP_TEST_STATE=)"
  if has_word "$found" "$RESET"; then ok "$name"
  else bad "$name" "$MIGRATE depends on [$found], not on $RESET"; fi
}

test_should_skip_the_reset_when_state_is_kept() {
  local name="test_should_skip_the_reset_when_state_is_kept" found
  found="$(prerequisites "$MIGRATE" KEEP_TEST_STATE=1)"
  if [ "$found" = "$INFRA" ]; then ok "$name"
  else bad "$name" "under KEEP_TEST_STATE=1 $MIGRATE depends on [$found], not only $INFRA"; fi
}

test_should_reset_and_migrate_an_owned_rig() {
  local name="test_should_reset_and_migrate_an_owned_rig" lane found
  for lane in "${BENCH_LANES[@]}"; do
    found="$(prerequisites "$lane" PROFILE=rig BENCH_TARGET_OWNED=owned)"
    if ! has_word "$found" "$MIGRATE"; then
      bad "$name" "$lane on an owned rig depends on [$found], without $MIGRATE"
      return
    fi
  done
  ok "$name"
}

test_should_leave_a_rig_it_does_not_own_alone() {
  local name="test_should_leave_a_rig_it_does_not_own_alone" lane found
  for lane in "${BENCH_LANES[@]}"; do
    found="$(prerequisites "$lane" PROFILE=rig "BENCH_TARGET_OWNED=owned no")"
    if has_word "$found" "$MIGRATE" || has_word "$found" "$RESET"; then
      bad "$name" "$lane on a rig owned by someone else depends on [$found]"
      return
    fi
  done
  ok "$name"
}

test_should_leave_a_deployed_profile_alone() {
  local name="test_should_leave_a_deployed_profile_alone" lane found
  for lane in "${BENCH_LANES[@]}"; do
    found="$(prerequisites "$lane" PROFILE=dev)"
    if has_word "$found" "$MIGRATE" || has_word "$found" "$RESET"; then
      bad "$name" "$lane on the dev profile depends on [$found]"
      return
    fi
  done
  ok "$name"
}

test_should_migrate_only_after_the_reset
test_should_skip_the_reset_when_state_is_kept
test_should_reset_and_migrate_an_owned_rig
test_should_leave_a_rig_it_does_not_own_alone
test_should_leave_a_deployed_profile_alone

if [ "$FAILURES" -ne 0 ]; then
  printf '%d failure(s)\n' "$FAILURES"
  exit 1
fi
echo "all reset-order cases passed"
