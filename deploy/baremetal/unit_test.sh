#!/usr/bin/env bash
# Self-test for the runner's systemd unit, agentsfleet-runner.service.
#
#     bash deploy/baremetal/unit_test.sh
#
# systemd reads the unit and says what it makes of it. `systemd-analyze verify`
# exits 0 over a misspelled directive or a value it cannot parse: it warns
# ("Unknown key name … ignoring") and drops the line, so a unit that loses its
# restart policy or its cgroup delegation still verifies. So the case fails on
# any output as well as on a non-zero exit.
#
# It needs systemd: it SKIPS on a machine without it and hard-fails when CI is
# set, so the unit is always proven on the ubuntu-latest runners that gate a
# merge.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly RUNNER_UNIT="$SCRIPT_DIR/agentsfleet-runner.service"

passed=0
failed=0
skipped=0

ok()   { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad()  { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }
skip() { printf 'SKIP %s\n       %s\n' "$1" "$2"; skipped=$((skipped + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# ExecStart's binary is not installed on a test machine, and systemd reports a
# missing one, so the copy verified points it at one that is; every other
# directive is read as written.
test_unit_passes_systemd_analyze() {
  local name="test_unit_passes_systemd_analyze"
  local copy="$WORK_DIR/agentsfleet-runner.service" output status=0
  sed "s|^ExecStart=[^ ]*|ExecStart=$(command -v true)|" "$RUNNER_UNIT" >"$copy"
  output="$(systemd-analyze verify "$copy" 2>&1)" || status=$?
  if [[ "$status" -ne 0 ]]; then
    bad "$name" "systemd-analyze verify exited $status: $output"
  elif [[ -n "$output" ]]; then
    bad "$name" "systemd-analyze verify warned, so systemd ignores part of the unit: $output"
  else
    ok "$name"
  fi
}

if command -v systemd-analyze >/dev/null 2>&1; then
  test_unit_passes_systemd_analyze
elif [[ -n "${CI:-}" ]]; then
  bad "test_unit_passes_systemd_analyze" "systemd-analyze not found on a CI runner — the unit must be proven here"
else
  skip "test_unit_passes_systemd_analyze" "systemd-analyze not installed (it ships with systemd)"
fi

printf '\n%d passed, %d failed, %d skipped\n' "$passed" "$failed" "$skipped"
[[ "$failed" -eq 0 ]]
