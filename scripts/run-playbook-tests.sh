#!/usr/bin/env bash
# Run every playbooks/**/*_test.sh concurrently, bounded, with ordered output.
#
# Each test file is a self-contained bash process — own mktemp work_dir, own
# stub PATH, own trap cleanup — so nothing about correctness depends on the
# order they run in. Running them serially cost the sum of all of them (~180s
# measured) to prove something the slowest single file already bounds.
#
# Bounded, not unbounded: these tests are fork-heavy rather than CPU-heavy (a
# PATH-stubbed `op`/`curl` is a bash script, so every intercepted call is a
# fork+exec). Twenty-one at once is fine on a workstation and thrashes a
# 2-core Continuous Integration (CI) runner. One job per core, which xargs -P
# does natively — no hand-rolled pid array, and no `wait -n` (bash 4.3+, while
# macOS still ships bash 3.2).
#
# Output is captured per file and replayed in sorted order once everything
# finishes; interleaved live output from N concurrent suites is unreadable.
#
# PLAYBOOK_TEST_JOBS  override the concurrency (default: one per core)
# PLAYBOOK_TEST_SCRUB `env` flags clearing ambient config, set by the Makefile
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

jobs="${PLAYBOOK_TEST_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}"
export PLAYBOOK_TEST_SCRUB="${PLAYBOOK_TEST_SCRUB:-}"

log_dir="$(mktemp -d)"
cleanup() { rm -rf -- "$log_dir"; }
trap cleanup EXIT INT TERM

# `while read` rather than mapfile: mapfile is bash 4+, and this has to run on
# whatever bash a CI image happens to ship.
tests=()
while IFS= read -r test_script; do
  tests+=("$test_script")
done < <(find playbooks -type f -name '*_test.sh' | sort)

if [ "${#tests[@]}" -eq 0 ]; then
  echo "✗ [playbooks] no shell regression tests found" >&2
  exit 1
fi

# The worker records failure as a marker FILE rather than leaning on an exit
# code: xargs collapses any number of failed children into a single status
# 123, which cannot say how many suites failed or which ones.
printf '%s\n' "${tests[@]}" \
  | xargs -P "$jobs" -I{} bash -c '
      test_script="$1"
      log="$2/$(printf "%s" "$test_script" | tr "/" "_")"
      printf "  %s\n" "$test_script" > "$log"
      # shellcheck disable=SC2086  # deliberate split: a list of `-u VAR` flags
      if ! env $PLAYBOOK_TEST_SCRUB bash "$test_script" >> "$log" 2>&1; then
        : > "$log.failed"
      fi
    ' _ {} "$log_dir" || true

failed=0
for test_script in "${tests[@]}"; do
  log="$log_dir/$(printf '%s' "$test_script" | tr '/' '_')"
  if [ -f "$log" ]; then
    cat "$log"
  fi
  if [ -f "$log.failed" ]; then
    failed=$((failed + 1))
  fi
done

if [ "$failed" -ne 0 ]; then
  printf '\n✗ [playbooks] %d regression test file(s) failed\n' "$failed" >&2
  exit 1
fi
