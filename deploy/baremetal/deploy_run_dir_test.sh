#!/usr/bin/env bash
# Self-tests for the run directory deploy.sh is handed: it installs from that
# directory alone, and removes it however the deploy ends.
#
#     bash deploy/baremetal/deploy_run_dir_test.sh
#
# Each case sources deploy.sh in a fresh subshell, as deploy_test.sh does. The
# run directories sit in this suite's scratch directory, which is not under
# HOST_RUNS_DIR, so a case that runs main tells check_run_dir it is; the check
# itself has cases of its own. `install` is stubbed on PATH and records every
# call, and creates the directories it is asked to, so a case can read where
# each installed file came from. `systemctl` and `cp` are stubbed to do nothing.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPLOY_SH="$SCRIPT_DIR/deploy.sh"
# shellcheck source=./deploy_test_support.sh
source "$SCRIPT_DIR/deploy_test_support.sh"
readonly LOCK_FREE=0
readonly LOCK_HELD=1
readonly HEALTHY=0
readonly UNHEALTHY=1

passed=0
failed=0

ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
readonly STUB_DIR="$WORK_DIR/bin"
readonly INSTALL_LOG="$WORK_DIR/install-calls"
mkdir -p "$STUB_DIR"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

cat >"$STUB_DIR/install" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$INSTALL_LOG"
if [ "$1" = -d ]; then
  for last; do :; done
  mkdir -p "$last"
fi
STUB
printf '#!/usr/bin/env bash\nexit 0\n' >"$STUB_DIR/systemctl"
printf '#!/usr/bin/env bash\nexit 0\n' >"$STUB_DIR/cp"
chmod +x "$STUB_DIR/install" "$STUB_DIR/systemctl" "$STUB_DIR/cp"

# Runs deploy.sh's main on `run_dir`, with the lock and the health check
# answering as given, and the run directory taken as under HOST_RUNS_DIR.
deploy_status() {
  local run_dir="$1" lock_status="$2" health="$3"
  : >"$INSTALL_LOG"
  (
    export PATH="$STUB_DIR:$PATH" INSTALL_LOG
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    acquire_deploy_lock() { [[ "$lock_status" -eq "$LOCK_FREE" ]] || exit "$lock_status"; }
    check_run_dir() { :; }
    verify_healthy() { return "$health"; }
    main runner v9.9.9 "$run_dir" >/dev/null 2>&1
  )
}

# The source of every file the last deploy installed, one per line.
installed_sources() {
  awk '$1 != "-d" { print $(NF - 1) }' "$INSTALL_LOG"
}

# Two run directories sit side by side when two deploys reach one host at
# once. The deploy handed one installs that one's binary, toolbox set and env
# file, nothing from the other, and removes only its own when it is done.
test_deploy_installs_only_from_the_run_directory_it_was_given() {
  local name="test_deploy_installs_only_from_the_run_directory_it_was_given"
  local mine="$WORK_DIR/${RUN_NAME_PREFIX}mine" other="$WORK_DIR/${RUN_NAME_PREFIX}other"
  local home="$WORK_DIR/storage-home" expected source
  run_dir_with "$mine" "" "RUNNER_STORAGE_HOME=$home"
  run_dir_with "$other" "" "RUNNER_STORAGE_HOME=$home"
  if ! deploy_status "$mine" "$LOCK_FREE" "$HEALTHY"; then
    bad "$name" "a deploy of a complete run directory failed"
    return
  fi
  for expected in "$RUN_BINARY_FILE" "$RUN_ENV_FILE" "${TOOLBOX_PARTS[@]/#/toolbox-$FIXTURE_DIGEST.}"; do
    if ! installed_sources | grep -qxF "$mine/$expected"; then
      bad "$name" "$mine/$expected was never installed"
      return
    fi
  done
  while IFS= read -r source; do
    if [[ "$source" != "$mine"/* ]]; then
      bad "$name" "the deploy handed $mine installed $source"
      return
    fi
  done < <(installed_sources)
  if [[ -e "$mine" ]]; then
    bad "$name" "$mine outlived the deploy that installed from it"
  elif [[ ! -f "$other/$RUN_BINARY_FILE" ]]; then
    bad "$name" "the deploy removed $other, which another deploy staged"
  else
    ok "$name"
  fi
}

# A deploy that fails after it staged, here at the health check, removes its
# run directory all the same.
test_deploy_that_fails_after_staging_removes_its_run_directory() {
  local name="test_deploy_that_fails_after_staging_removes_its_run_directory"
  local run_dir="$WORK_DIR/${RUN_NAME_PREFIX}unhealthy"
  run_dir_with "$run_dir"
  if deploy_status "$run_dir" "$LOCK_FREE" "$UNHEALTHY"; then
    bad "$name" "a deploy whose runner never came up exited 0"
  elif ! installed_sources | grep -qxF "$run_dir/$RUN_BINARY_FILE"; then
    bad "$name" "the deploy stopped before it installed the binary — test harness fault, not a deploy fault"
  elif [[ -e "$run_dir" ]]; then
    bad "$name" "$run_dir outlived the deploy that failed"
  else
    ok "$name"
  fi
}

# The deploy that loses the lock to another installs nothing, removes its own
# run directory, and leaves the other deploy's alone.
test_deploy_refused_the_lock_removes_only_its_own_run_directory() {
  local name="test_deploy_refused_the_lock_removes_only_its_own_run_directory"
  local mine="$WORK_DIR/${RUN_NAME_PREFIX}locked-out" holder="$WORK_DIR/${RUN_NAME_PREFIX}holder"
  run_dir_with "$mine"
  run_dir_with "$holder"
  if deploy_status "$mine" "$LOCK_HELD" "$HEALTHY"; then
    bad "$name" "a deploy refused the lock exited 0"
  elif [[ -s "$INSTALL_LOG" ]]; then
    bad "$name" "a deploy refused the lock installed: $(head -1 "$INSTALL_LOG")"
  elif [[ -e "$mine" ]]; then
    bad "$name" "$mine outlived the deploy refused the lock"
  elif [[ ! -f "$holder/$RUN_BINARY_FILE" ]]; then
    bad "$name" "the deploy refused the lock removed $holder, the lock holder's"
  else
    ok "$name"
  fi
}

# main removes the run directory as root, so it first refuses one that is not
# under HOST_RUNS_DIR, and then removes nothing. This scratch directory is not.
test_deploy_refuses_a_run_directory_outside_the_runs_root() {
  local name="test_deploy_refuses_a_run_directory_outside_the_runs_root"
  local run_dir="$WORK_DIR/${RUN_NAME_PREFIX}elsewhere"
  run_dir_with "$run_dir"
  : >"$INSTALL_LOG"
  if (
    export PATH="$STUB_DIR:$PATH" INSTALL_LOG
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    set +e
    acquire_deploy_lock() { :; }
    main runner v9.9.9 "$run_dir" >/dev/null 2>&1
  ); then
    bad "$name" "a deploy handed $run_dir exited 0"
  elif [[ -s "$INSTALL_LOG" ]]; then
    bad "$name" "a deploy handed a directory outside the runs root installed from it"
  elif [[ ! -f "$run_dir/$RUN_BINARY_FILE" ]]; then
    bad "$name" "a deploy removed $run_dir, which is not under the runs root"
  else
    ok "$name"
  fi
}

# Status of check_run_dir for `run_dir` against `runs_root`.
run_dir_status() {
  (
    # shellcheck source=./deploy.sh
    source "$DEPLOY_SH" >/dev/null 2>&1
    check_run_dir "$1" "$2" >/dev/null 2>&1
  )
}

test_check_run_dir_takes_only_a_run_directory_directly_under_the_root() {
  local name="test_check_run_dir_takes_only_a_run_directory_directly_under_the_root"
  local root="$WORK_DIR/runs" run elsewhere="$WORK_DIR/elsewhere" refused
  run="$root/${RUN_NAME_PREFIX}abc123"
  mkdir -p "$run" "$root/other" "$elsewhere/${RUN_NAME_PREFIX}abc123"
  if ! run_dir_status "$run" "$root"; then
    bad "$name" "$run was refused"
    return
  fi
  for refused in "$root/other" "$root/${RUN_NAME_PREFIX}missing" "$run/../other" \
    "$elsewhere/${RUN_NAME_PREFIX}abc123" "$root" "$root/$RUN_NAME_PREFIX"; do
    if run_dir_status "$refused" "$root"; then
      bad "$name" "$refused passed as a run directory under $root"
      return
    fi
  done
  ok "$name"
}

test_deploy_installs_only_from_the_run_directory_it_was_given
test_deploy_that_fails_after_staging_removes_its_run_directory
test_deploy_refused_the_lock_removes_only_its_own_run_directory
test_deploy_refuses_a_run_directory_outside_the_runs_root
test_check_run_dir_takes_only_a_run_directory_directly_under_the_root

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
