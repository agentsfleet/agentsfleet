#!/usr/bin/env bash
# Self-tests for bench_cutover.sh — the budget refusal and the verdict.
#
#     bash scripts/bench_cutover_test.sh
#
# BENCH_HEY and BENCH_RSS_PROBE are substituted with fixtures, so the suite
# needs neither a load generator nor a container and runs in about a second —
# which is what lets it ride `make lint-all`. What it grades is the half that
# decides: whether a budget is present, and whether a measurement passes it.
#
# Covers Dimension 4.2's `test_bench_cutover_refuses_unset_budget`.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly LANE="$SCRIPT_DIR/bench_cutover.sh"

readonly BASELINE_URL="http://bench-baseline.invalid"
readonly CANDIDATE_URL="http://bench-candidate.invalid"

# The fixture's latencies, in milliseconds, and the budgets they are graded by.
# A candidate 10% slower passes a 20% tolerance and fails a 5% one, so the two
# verdict cases differ ONLY in the budget — not in the measurement.
readonly FIXTURE_BASELINE_MS=100
readonly FIXTURE_CANDIDATE_MS=110
readonly TOLERANCE_ADMITTING=20
readonly TOLERANCE_REFUSING=5
readonly RSS_CEILING_ADMITTING=256
readonly RSS_CEILING_REFUSING=64
readonly FIXTURE_RSS_MB=128

passed=0
failed=0
ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# A load generator that emits hey's CSV shape: a header, then one row per
# sample whose first column is the response time in SECONDS. The latency is
# chosen by which URL it was pointed at, so one fixture serves both sides of a
# comparison.
cat >"$WORK_DIR/hey" <<'HEY'
#!/usr/bin/env bash
set -uo pipefail
url="${*: -1}"
case "$url" in
  *bench-candidate*) latency_ms="$FIXTURE_CANDIDATE_MS" ;;
  *)                 latency_ms="$FIXTURE_BASELINE_MS" ;;
esac
[ "${FIXTURE_EMPTY:-0}" = "1" ] && { printf 'response-time,DNS+dialup\n'; exit 0; }
printf 'response-time,DNS+dialup\n'
for _ in $(seq 1 100); do
  awk -v ms="$latency_ms" 'BEGIN { printf "%.6f,0.0\n", ms / 1000 }'
done
HEY
chmod +x "$WORK_DIR/hey"

cat >"$WORK_DIR/rss" <<'RSS'
#!/usr/bin/env bash
[ "${FIXTURE_RSS_UNREADABLE:-0}" = "1" ] && exit 0
printf '%s' "${FIXTURE_RSS_MB:-0}"
RSS
chmod +x "$WORK_DIR/rss"

# Runs the lane. Every argument is a VAR=value; `env` rather than an assignment
# prefix, because a prefix is parsed before "$@" expands.
run_lane() {
  env BENCH_HEY="$WORK_DIR/hey" BENCH_RSS_PROBE="$WORK_DIR/rss" \
    FIXTURE_BASELINE_MS="$FIXTURE_BASELINE_MS" \
    FIXTURE_CANDIDATE_MS="$FIXTURE_CANDIDATE_MS" \
    FIXTURE_RSS_MB="$FIXTURE_RSS_MB" \
    BENCH_DURATION_SEC=1 BENCH_CONCURRENCY=1 \
    "$@" bash "$LANE" >"$WORK_DIR/out" 2>&1
  printf '%s' "$?"
}
lane_output() { cat "$WORK_DIR/out"; }

# --------------------------------------------------------------------------
# test_bench_cutover_refuses_unset_budget — the row this lane exists for.
#
# A benchmark whose thresholds are absent does not fail. It measures, prints,
# and returns success, which reads exactly like a passing gate — so each of
# these asserts the refusal NAMES the constant, not merely that it exited.
# --------------------------------------------------------------------------
refusal_case() {
  local label="$1" expected_name="$2"; shift 2
  local status
  status="$(run_lane "$@")"
  if [ "$status" = "0" ]; then
    bad "$label" "expected a non-zero exit, got 0: $(lane_output)"
  elif ! lane_output | grep -qF "$expected_name"; then
    bad "$label" "failed without naming $expected_name: $(lane_output)"
  else
    ok "$label"
  fi
}

refusal_case "both budgets unset — the lane refuses naming the tolerance" \
  BENCH_P95_TOLERANCE_PCT BASE_URL="$BASELINE_URL"
refusal_case "both budgets unset — the lane refuses naming the ceiling" \
  BENCH_RSS_CEILING_MB BASE_URL="$BASELINE_URL"
refusal_case "the tolerance alone unset is refused" BENCH_P95_TOLERANCE_PCT \
  BASE_URL="$BASELINE_URL" BENCH_RSS_CEILING_MB="$RSS_CEILING_ADMITTING"
refusal_case "the ceiling alone unset is refused" BENCH_RSS_CEILING_MB \
  BASE_URL="$BASELINE_URL" BENCH_P95_TOLERANCE_PCT="$TOLERANCE_ADMITTING"
refusal_case "an empty budget is refused, not read as zero" BENCH_P95_TOLERANCE_PCT \
  BASE_URL="$BASELINE_URL" BENCH_P95_TOLERANCE_PCT="" \
  BENCH_RSS_CEILING_MB="$RSS_CEILING_ADMITTING"
refusal_case "a non-numeric budget is refused" BENCH_RSS_CEILING_MB \
  BASE_URL="$BASELINE_URL" BENCH_P95_TOLERANCE_PCT="$TOLERANCE_ADMITTING" \
  BENCH_RSS_CEILING_MB="lots"

# The budgets are graded before BASE_URL, so a lane with neither reports the
# budgets — the fault a caller can actually fix without standing anything up.
status="$(run_lane)"
if [ "$status" != "0" ] && lane_output | grep -qF "BENCH_P95_TOLERANCE_PCT is unset"; then
  ok "with nothing set at all, the budgets are what the lane reports"
else
  bad "with nothing set at all, the budgets are what the lane reports" "$(lane_output)"
fi

# --------------------------------------------------------------------------
# ...and passes with both set.
# --------------------------------------------------------------------------
status="$(run_lane BASE_URL="$BASELINE_URL" \
  BENCH_P95_TOLERANCE_PCT="$TOLERANCE_ADMITTING" \
  BENCH_RSS_CEILING_MB="$RSS_CEILING_ADMITTING")"
if [ "$status" = "0" ] && lane_output | grep -qF "p95_ms=$FIXTURE_BASELINE_MS"; then
  ok "record mode runs with both budgets set and prints the measurement"
else
  bad "record mode runs with both budgets set" "exit $status: $(lane_output)"
fi

if lane_output | grep -qF "no verdict reached"; then
  ok "record mode says it reached no verdict rather than implying one"
else
  bad "record mode says it reached no verdict" "$(lane_output)"
fi

# --------------------------------------------------------------------------
# COMPARE — the verdict. The two latency cases differ only in the budget, so
# what they grade is the decision and not the measurement.
# --------------------------------------------------------------------------
compare() {
  run_lane BASE_URL="$BASELINE_URL" COMPARE_URL="$CANDIDATE_URL" \
    BENCH_P95_TOLERANCE_PCT="$1" BENCH_RSS_CEILING_MB="$2" "${@:3}"
}

status="$(compare "$TOLERANCE_ADMITTING" "$RSS_CEILING_ADMITTING")"
if [ "$status" = "0" ]; then
  ok "a candidate inside the tolerance passes"
else
  bad "a candidate inside the tolerance passes" "exit $status: $(lane_output)"
fi

status="$(compare "$TOLERANCE_REFUSING" "$RSS_CEILING_ADMITTING")"
if [ "$status" != "0" ] && lane_output | grep -qF "BENCH_P95_TOLERANCE_PCT=$TOLERANCE_REFUSING"; then
  ok "a candidate outside the tolerance fails naming the budget"
else
  bad "a candidate outside the tolerance fails naming the budget" "exit $status: $(lane_output)"
fi

status="$(compare "$TOLERANCE_ADMITTING" "$RSS_CEILING_REFUSING")"
if [ "$status" != "0" ] && lane_output | grep -qF "BENCH_RSS_CEILING_MB=$RSS_CEILING_REFUSING"; then
  ok "a resident set over the ceiling fails naming the budget"
else
  bad "a resident set over the ceiling fails naming the budget" "exit $status: $(lane_output)"
fi

# A ceiling that cannot be read is the "grades nothing" case in its purest
# form: everything is configured, the lane runs, and the RSS budget silently
# decides nothing. It has to be a failure, not a quiet pass.
status="$(compare "$TOLERANCE_ADMITTING" "$RSS_CEILING_ADMITTING" FIXTURE_RSS_UNREADABLE=1)"
if [ "$status" != "0" ] && lane_output | grep -qF "grades nothing"; then
  ok "a declared ceiling with no readable resident set fails rather than passing quietly"
else
  bad "a declared ceiling with no readable resident set fails" "exit $status: $(lane_output)"
fi

# A load generator that measured nothing exits 0 and prints a header. The lane
# has to notice, for the same reason the integration lane counts passing tests.
status="$(run_lane BASE_URL="$BASELINE_URL" \
  BENCH_P95_TOLERANCE_PCT="$TOLERANCE_ADMITTING" \
  BENCH_RSS_CEILING_MB="$RSS_CEILING_ADMITTING" FIXTURE_EMPTY=1)"
if [ "$status" != "0" ] && lane_output | grep -qF "zero samples"; then
  ok "zero samples fails — a run that measured nothing is not a pass"
else
  bad "zero samples fails" "exit $status: $(lane_output)"
fi

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ] || exit 1
[ "$passed" -gt 0 ] || { printf 'FAIL the self-test suite ran nothing\n' >&2; exit 1; }
