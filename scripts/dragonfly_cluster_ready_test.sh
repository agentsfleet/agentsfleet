#!/usr/bin/env bash
# dragonfly_cluster_ready_test.sh — the cluster-ready check's own tests.
#
# Pure: flyctl is injected, so this runs with no Fly account and no network.
# Invoked by `make lint-scripts`, because a check whose own tests never run is
# enforcement in appearance only.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK="$HERE/dragonfly_cluster_ready.sh"
readonly HERE CHECK
FAILURES=0

ok()  { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); }

# A fake flyctl whose exit code the caller chooses. Written per test, so a test
# wanting a failure cannot be confused by one that wanted a success.
fake_flyctl() {
  local exit_code="$1" dir
  dir="$(mktemp -d "${TMPDIR:-/tmp}/cluster-ready.XXXXXX")"
  printf '#!/usr/bin/env bash\nexit %s\n' "$exit_code" >"$dir/flyctl"
  chmod +x "$dir/flyctl"
  printf '%s/flyctl' "$dir"
}

test_should_pass_when_healthy_answers() {
  local name="test_should_pass_when_healthy_answers"
  local fake; fake="$(fake_flyctl 0)"
  if FLYCTL="$fake" bash "$CHECK" dragonfly-dev >/dev/null 2>&1; then
    ok "$name"
  else
    bad "$name" "a healthy cluster was reported as not ready"
  fi
  rm -rf "$(dirname "$fake")"
}

# The whole point: a node can listen without ever bootstrapping, and the check
# must not agree with Fly's TCP probe about that.
test_should_fail_when_healthy_refuses() {
  local name="test_should_fail_when_healthy_refuses"
  local fake; fake="$(fake_flyctl 1)"
  if FLYCTL="$fake" bash "$CHECK" dragonfly-dev >/dev/null 2>&1; then
    bad "$name" "a cluster that never bootstrapped was reported ready"
  else
    ok "$name"
  fi
  rm -rf "$(dirname "$fake")"
}

test_should_refuse_a_missing_app_argument() {
  local name="test_should_refuse_a_missing_app_argument"
  bash "$CHECK" >/dev/null 2>&1
  if [ "$?" -eq 2 ]; then
    ok "$name"
  else
    bad "$name" "a call with no app did not exit 2"
  fi
}

test_should_pass_when_healthy_answers
test_should_fail_when_healthy_refuses
test_should_refuse_a_missing_app_argument

if [ "$FAILURES" -ne 0 ]; then
  printf '%s failure(s)\n' "$FAILURES"
  exit 1
fi
printf '\n%s passed, 0 failed\n' 3
