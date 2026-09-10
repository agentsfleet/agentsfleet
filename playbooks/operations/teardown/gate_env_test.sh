#!/usr/bin/env bash
#
# The teardown gates' ENV contract, asserted at the gate.
#
# Both 00_gate.sh files advertise `ENV=dev|prod` and present themselves as the
# gate, but until this test they only checked that ENV was NON-EMPTY.
# `ENV=staging` passed 00_gate.sh and was refused one or two steps later — by
# 01_credential_check.sh's ALLOW_*_TEARDOWN check, or by 02_teardown.sh's own
# `dev|prod` validation. Nothing destructive could run on an unvalidated ENV,
# because those steps validate before they act; what was wrong is that the
# entry point did not mean what its own usage line said, so a reader or a
# future dispatched step would have been entitled to trust a check that had
# not happened.
#
# Each case runs a COPY of the gate beside stub steps, so a pass proves the
# gate dispatched and proves nothing about any real database or cache. No case
# sets ALLOW_DATABASE_TEARDOWN or ALLOW_REDIS_TEARDOWN, and no case executes
# the real 02_teardown.sh.
#
# The negatives assert BOTH a non-zero exit AND that no step ran. A non-zero
# exit on its own is exactly what made the old, weaker gate look correct: the
# refusal came from downstream, so the exit code could not tell the two apart.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE_DIRS=(database redis)
STEPS=(01_credential_check.sh 02_teardown.sh 03_verify.sh)

passed=0
failed=0
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

# A copy of one gate beside stub steps that record the order they ran in.
stage_gate() {
  local gate="$1"
  local case_dir="$2"
  local step
  mkdir -p "$case_dir"
  cp "$gate" "$case_dir/00_gate.sh"
  chmod +x "$case_dir/00_gate.sh"
  for step in "${STEPS[@]}"; do
    printf '%s\n' \
      '#!/usr/bin/env bash' \
      'printf '\''%s\n'\'' "${0##*/}" >>"$CALLS"' \
      >"$case_dir/$step"
    chmod +x "$case_dir/$step"
  done
  : >"$case_dir/calls"
}

# An ENV the gate must refuse itself, naming both accepted values, with no step
# dispatched. `mode` is `set` or `unset`: an unset ENV and an empty ENV are
# different operator mistakes and both have to be refused.
assert_rejected() {
  local name="$1"
  local gate="$2"
  local mode="$3"
  local env_value="${4-}"
  local case_dir="$work_dir/$name"
  local output status=0

  stage_gate "$gate" "$case_dir"
  case "$mode" in
  unset)
    output="$(env -u ENV CALLS="$case_dir/calls" bash "$case_dir/00_gate.sh" 2>&1)" ||
      status=$?
    ;;
  set)
    output="$(ENV="$env_value" CALLS="$case_dir/calls" bash "$case_dir/00_gate.sh" 2>&1)" ||
      status=$?
    ;;
  *)
    bad "$name" "unknown mode '$mode'"
    return
    ;;
  esac

  if [ "$status" -eq 0 ]; then
    bad "$name" "gate accepted ENV ($mode '$env_value')"
  elif [ -s "$case_dir/calls" ]; then
    bad "$name" \
      "gate dispatched a step on ENV ($mode '$env_value'): $(paste -sd ' ' "$case_dir/calls")"
  elif [[ "$output" != *dev* ]] || [[ "$output" != *prod* ]]; then
    bad "$name" "rejection does not name the accepted values: $output"
  else
    ok "$name"
  fi
}

# The positive control, and the reason the refusals above are attributable to
# the ENV check: the same staged gate must still dispatch all three steps in
# order when ENV is one of the accepted values.
assert_accepted() {
  local name="$1"
  local gate="$2"
  local env_value="$3"
  local case_dir="$work_dir/$name"
  local output status=0 actual expected

  stage_gate "$gate" "$case_dir"
  output="$(ENV="$env_value" CALLS="$case_dir/calls" bash "$case_dir/00_gate.sh" 2>&1)" ||
    status=$?
  actual="$(paste -sd ' ' "$case_dir/calls")"
  # Default IFS begins with a space, so `[*]` joins the step list with one.
  expected="${STEPS[*]}"

  if [ "$status" -ne 0 ]; then
    bad "$name" "gate rejected ENV=$env_value: $output"
  elif [ "$actual" != "$expected" ]; then
    bad "$name" "expected steps '$expected'; got '$actual'"
  else
    ok "$name"
  fi
}

for gate_dir in "${GATE_DIRS[@]}"; do
  gate="$SCRIPT_DIR/$gate_dir/00_gate.sh"
  if [ ! -x "$gate" ]; then
    bad "${gate_dir}_gate_is_executable" "missing or not executable: $gate"
    continue
  fi
  assert_rejected "${gate_dir}_rejects_unknown_env" "$gate" set staging
  assert_rejected "${gate_dir}_rejects_all_env" "$gate" set all
  assert_rejected "${gate_dir}_rejects_blank_env" "$gate" set ""
  assert_rejected "${gate_dir}_rejects_unset_env" "$gate" unset
  assert_accepted "${gate_dir}_accepts_dev" "$gate" dev
  assert_accepted "${gate_dir}_accepts_prod" "$gate" prod
done

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
