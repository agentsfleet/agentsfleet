#!/usr/bin/env bash
# Regression tests for what the runner deploy lane stages for the host.
#
#     bash playbooks/lib/runner/runner_stage_test.sh
#
# Runs on the stub harness in runner_test_support.sh, as runner_test.sh does.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./runner_test_support.sh
source "$SCRIPT_DIR/runner_test_support.sh"

# A mktemp that makes what the real one makes and records each path, so a case
# can look for the file afterwards. macOS mktemp ignores TMPDIR, so a case
# cannot find the file by pointing TMPDIR at a directory of its own.
mktemp_stub_dir="$work_dir/mktemp-bin"
mktemp_log="$work_dir/mktemp-paths"
mkdir -p "$mktemp_stub_dir"
cat >"$mktemp_stub_dir/mktemp" <<STUB
#!/usr/bin/env bash
made="\$($(command -v mktemp) "\$@")" || exit
printf '%s\n' "\$made" >>"$mktemp_log"
printf '%s\n' "\$made"
STUB
chmod +x "$mktemp_stub_dir/mktemp"

# The runner's environment is written locally before its copy to the host, and
# it carries the token: a copy that fails must not leave it behind.
test_should_leave_no_env_file_when_its_copy_fails() {
  local name="test_should_leave_no_env_file_when_its_copy_fails"
  local output status=0 made
  : >"$mktemp_log"
  output="$(
    run_script ENV=dev PATH="$mktemp_stub_dir:$stub_dir:$PATH" \
      TAILSCALE_FAIL_MATCH='.env.new' bash "$DEPLOY"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a deploy whose env copy failed exited 0"
    return
  elif ! grep -q "cat > '[^']*\.env\.new'" "$calls" || [ ! -s "$mktemp_log" ]; then
    bad "$name" "the deploy stopped before it wrote and copied the env file — test harness fault: $output"
    return
  fi
  while IFS= read -r made; do
    # The run directory the stub host made goes through this mktemp too, and
    # test_should_remove_its_run_directory_when_staging_fails answers for it.
    if [[ "$made" != "$host_root"/* && -e "$made" ]]; then
      bad "$name" "the env file $made outlived the failed copy"
      return
    fi
  done <"$mktemp_log"
  ok "$name"
}

# The run directory the host's deploy.sh was handed, as the host names it.
handed_run_dir() {
  grep -o "deploy\.sh runner '[^']*' [^[:space:]]*" "$calls" | awk '{ print $NF }' | tail -1
}

# The run directory the binary was copied into, as the host names it.
staged_run_dir() {
  grep -o "cat > '$HOST_RUNS_PATH/[^/']*" "$calls" | head -1 | sed "s/^cat > '//"
}

# Where a host path lives under the stub's stand-in for HOST_ROOT.
on_stub_host() {
  printf '%s\n' "$host_root${1#"$HOST_ROOT_PATH"}"
}

# Prints the first file the last run copied anywhere but the deploy files'
# directory and `run_dir`, and fails when there is none.
first_write_outside() {
  local run_dir="$1" dest
  while IFS= read -r dest; do
    case "$dest" in
      "$HOST_DEPLOY_PATH"/* | "$run_dir"/*) ;;
      *) printf '%s\n' "$dest"; return 0 ;;
    esac
  done < <(grep -o "cat > '[^']*'" "$calls" | sed "s/^cat > '//; s/'\$//")
  return 1
}

# Fails, naming what is missing, unless `run_dir` on the stub host holds this
# deploy's binary, its toolbox set, and its environment readable by the deploy
# user alone.
holds_one_deploy() {
  local dir part
  dir="$(on_stub_host "$1")"
  [ -f "$dir/$RUN_BINARY_NAME" ] || { echo "no binary in $1"; return 1; }
  for part in erofs json json.sig; do
    [ -f "$dir/toolbox-$TOOLBOX_FIXTURE_DIGEST.$part" ] || { echo "no .$part in $1"; return 1; }
  done
  grep -q '^AGENTSFLEET_RUNNER_TOKEN=agt_rREAL_TOKEN$' "$dir/$RUN_ENV_NAME" 2>/dev/null ||
    { echo "no runner token in $1/$RUN_ENV_NAME"; return 1; }
  [ -n "$(find "$dir/$RUN_ENV_NAME" -perm 0600)" ] ||
    { echo "$1/$RUN_ENV_NAME is readable beyond the deploy user"; return 1; }
}

# Two deploys reaching one host at once must not write into each other's
# staging: each copies into the directory the host made for it, and hands the
# host's deploy.sh that directory.
test_should_stage_each_deploy_into_a_run_directory_of_its_own() {
  local name="test_should_stage_each_deploy_into_a_run_directory_of_its_own"
  local output dir dirs=() outside missing
  for _ in first second; do
    if ! output="$(run_script ENV=dev bash "$DEPLOY")"; then
      bad "$name" "$output"
      return
    fi
    dir="$(handed_run_dir)"
    if [ -z "$dir" ]; then
      bad "$name" "the host's deploy.sh was handed no run directory"
      return
    elif outside="$(first_write_outside "$dir")"; then
      bad "$name" "a deploy staged into $dir also wrote $outside"
      return
    elif ! missing="$(holds_one_deploy "$dir")"; then
      bad "$name" "$missing"
      return
    fi
    dirs+=("$dir")
  done
  if [ "${dirs[0]}" = "${dirs[1]}" ]; then
    bad "$name" "two deploys staged into one directory, ${dirs[0]}"
  else
    ok "$name"
  fi
}

# A deploy that stops after it staged, before the host's deploy.sh has the
# directory, removes it itself: nothing else would.
test_should_remove_its_run_directory_when_staging_fails() {
  local name="test_should_remove_its_run_directory_when_staging_fails"
  local output status=0 dir
  output="$(run_script ENV=dev TAILSCALE_FAIL_MATCH='.env.new' bash "$DEPLOY")" || status=$?
  dir="$(staged_run_dir)"
  if [ "$status" -eq 0 ]; then
    bad "$name" "a deploy whose env copy failed exited 0"
  elif [ -z "$dir" ]; then
    bad "$name" "the deploy stopped before it staged the binary — test harness fault: $output"
  elif [ -e "$(on_stub_host "$dir")" ]; then
    bad "$name" "$dir outlived the deploy that staged into it"
  else
    ok "$name"
  fi
}

# Once the host's deploy.sh has the directory it removes it when it ends. The
# playbook must leave it alone: one cancelled mid-deploy leaves that deploy
# running on the host, still reading the directory.
test_should_leave_the_run_directory_to_the_host_deploy_once_started() {
  local name="test_should_leave_the_run_directory_to_the_host_deploy_once_started"
  local output status=0 dir
  output="$(run_script ENV=dev TAILSCALE_FAIL_MATCH='deploy.sh runner' bash "$DEPLOY")" || status=$?
  dir="$(handed_run_dir)"
  if [ "$status" -eq 0 ]; then
    bad "$name" "a deploy whose host deploy failed exited 0"
  elif [ -z "$dir" ]; then
    bad "$name" "the deploy never reached the host's deploy.sh — test harness fault: $output"
  elif [ ! -d "$(on_stub_host "$dir")" ]; then
    bad "$name" "the playbook removed $dir after handing it to the host's deploy.sh"
  else
    ok "$name"
  fi
}

test_should_leave_no_env_file_when_its_copy_fails
test_should_stage_each_deploy_into_a_run_directory_of_its_own
test_should_remove_its_run_directory_when_staging_fails
test_should_leave_the_run_directory_to_the_host_deploy_once_started
report_results
