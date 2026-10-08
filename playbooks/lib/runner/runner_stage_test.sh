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
    if [ -e "$made" ]; then
      bad "$name" "the env file $made outlived the failed copy"
      return
    fi
  done <"$mktemp_log"
  ok "$name"
}

test_should_leave_no_env_file_when_its_copy_fails
report_results
