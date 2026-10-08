#!/usr/bin/env bash
# Self-tests for the runner's systemd service: the health check deploy.sh
# sources from service.sh, and the unit file itself.
#
#     bash deploy/baremetal/service_test.sh
#
# Each case sources service.sh, with the log.sh it logs through, in a fresh
# subshell and passes verify_healthy bounds that do not wait. A systemctl stub
# on PATH answers `is-active` from STUB_IS_ACTIVE and `show -p NRestarts` from
# STUB_NRESTARTS, one value per call with the last repeating, so a case can
# hand the check a unit that systemd restarts, or that is down, mid-window.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPLOY_SH="$SCRIPT_DIR/deploy.sh"
readonly SERVICE_SH="$SCRIPT_DIR/service.sh"
readonly LOG_SH="$SCRIPT_DIR/log.sh"
readonly RUNNER_UNIT="$SCRIPT_DIR/agentsfleet-runner.service"
# The stub answers for any unit, so the name only has to be passed through.
readonly UNIT_UNDER_TEST="agentsfleet-runner.service"

passed=0
failed=0
skipped=0

ok()   { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad()  { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }
skip() { printf 'SKIP %s\n       %s\n' "$1" "$2"; skipped=$((skipped + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
readonly STUB_DIR="$WORK_DIR/bin"
mkdir -p "$STUB_DIR"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

cat >"$STUB_DIR/systemctl" <<'STUB'
#!/usr/bin/env bash
# The next of the space-separated `list`, counted in `counter`, the last repeating.
next_of() {
  local list="$1" counter="$2" index=0
  local -a values
  read -r -a values <<<"$list"
  [ -f "$counter" ] && index="$(cat "$counter")"
  printf '%s\n' "$((index + 1))" >"$counter"
  [ "$index" -ge "${#values[@]}" ] && index=$((${#values[@]} - 1))
  printf '%s\n' "${values[index]}"
}
for arg in "$@"; do
  case "$arg" in
    is-active) exit "$(next_of "${STUB_IS_ACTIVE:-0}" "$STUB_STATE.active")" ;;
    is-failed) exit 1 ;;
    show) next_of "${STUB_NRESTARTS:-0}" "$STUB_STATE.restarts"; exit 0 ;;
  esac
done
exit 0
STUB
chmod +x "$STUB_DIR/systemctl"

# verify_healthy's status over one probe, no delay and no window, under the
# stub answers in "$@" (VAR=value pairs).
health_status() {
  (
    export PATH="$STUB_DIR:$PATH" STUB_STATE="$WORK_DIR/state-$RANDOM"
    local assignment
    for assignment in "$@"; do export "${assignment?}"; done
    # shellcheck source=./log.sh
    source "$LOG_SH"
    # shellcheck source=./service.sh
    source "$SERVICE_SH"
    verify_healthy "$UNIT_UNDER_TEST" 1 0 0 >/dev/null 2>&1
  )
}

# The failing cases assert a non-zero status, which a harness that cannot
# source service.sh also yields. Fail loud up front instead.
preflight() {
  (
    # shellcheck source=./log.sh
    source "$LOG_SH" && source "$SERVICE_SH" && declare -F verify_healthy >/dev/null
  ) || { printf 'FATAL preflight: sourcing service.sh did not define the health check\n' >&2; exit 2; }
}

preflight

# Restart=always cycles a runner that exits during boot back through active,
# so a single active reading proves nothing: systemd restarting it inside the
# window fails the deploy.
test_service_health_fails_a_runner_restarted_inside_the_window() {
  local name="test_service_health_fails_a_runner_restarted_inside_the_window"
  if health_status STUB_NRESTARTS="0 1"; then
    bad "$name" "a unit systemd restarted inside the window passed the health check"
  else
    ok "$name"
  fi
}

# A runner caught between an exit and its restart reads not-active at the end
# of the window, with the restart count not yet moved.
test_service_health_fails_a_runner_down_at_the_window_end() {
  local name="test_service_health_fails_a_runner_down_at_the_window_end"
  if health_status STUB_IS_ACTIVE="0 3" STUB_NRESTARTS="4"; then
    bad "$name" "a unit that was not active at the end of the window passed"
  else
    ok "$name"
  fi
}

test_service_health_passes_a_runner_that_stays_up() {
  local name="test_service_health_passes_a_runner_that_stays_up"
  if health_status STUB_NRESTARTS="2"; then
    ok "$name"
  else
    bad "$name" "a unit active throughout with no restart failed the health check"
  fi
}

# The window only sees a crash loop when it outlasts the unit's RestartSec.
test_service_health_window_outlasts_restart_sec() {
  local name="test_service_health_window_outlasts_restart_sec"
  local restart_sec window
  restart_sec="$(sed -n 's/^RestartSec=\([0-9]*\)s\{0,1\}$/\1/p' "$RUNNER_UNIT" | head -1)"
  # shellcheck source=./deploy.sh
  window="$(source "$DEPLOY_SH" >/dev/null 2>&1; printf '%s' "$HEALTH_STABLE_SECONDS")"
  if [[ -z "$restart_sec" ]]; then
    bad "$name" "RestartSec in $RUNNER_UNIT is not a whole number of seconds"
  elif [[ "$window" -le "$restart_sec" ]]; then
    bad "$name" "HEALTH_STABLE_SECONDS=$window does not outlast RestartSec=$restart_sec"
  else
    ok "$name"
  fi
}

# systemd reads the unit and says what it makes of it. `systemd-analyze verify`
# exits 0 over a misspelled directive or a value it cannot parse: it warns
# ("Unknown key name … ignoring") and drops the line, so a unit that loses its
# restart policy or its cgroup delegation still verifies. So the case fails on
# any output as well as on a non-zero exit. ExecStart's binary is not installed
# on a test machine, and systemd reports a missing one, so the copy verified
# points it at one that is; every other directive is read as written.
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

test_service_health_fails_a_runner_restarted_inside_the_window
test_service_health_fails_a_runner_down_at_the_window_end
test_service_health_passes_a_runner_that_stays_up
test_service_health_window_outlasts_restart_sec
# The unit check needs systemd: it skips on a machine without it and fails when
# CI is set, so the unit is always proven on the ubuntu-latest runners that gate
# a merge.
if command -v systemd-analyze >/dev/null 2>&1; then
  test_unit_passes_systemd_analyze
elif [[ -n "${CI:-}" ]]; then
  bad "test_unit_passes_systemd_analyze" "systemd-analyze not found on a CI runner — the unit must be proven here"
else
  skip "test_unit_passes_systemd_analyze" "systemd-analyze not installed (it ships with systemd)"
fi

printf '\n%d passed, %d failed, %d skipped\n' "$passed" "$failed" "$skipped"
[[ "$failed" -eq 0 ]]
