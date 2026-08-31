#!/usr/bin/env bash
# The cutover benchmark lane. Run via `make bench-cutover`, or directly:
#
#     BASE_URL=http://127.0.0.1:8080 COMPARE_URL=http://127.0.0.1:3000 \
#       BENCH_P95_TOLERANCE_PCT=20 BENCH_RSS_CEILING_MB=192 \
#       bash scripts/bench_cutover.sh
#
# Tests covered:
#   * test_bench_cutover_refuses_unset_budget — either budget unset, empty or
#     non-numeric exits non-zero naming the constant; both set runs
#
# WHY THE BUDGETS REFUSE TO BE UNSET. A benchmark lane whose thresholds are
# absent does not fail — it measures, prints, and returns success, which reads
# exactly like a passing gate. The swap this milestone prepares for is gated on
# this command, so "grades nothing" and "grades green" must never look alike.
# Both budgets are asserted before a single request is sent.
#
# THE BUDGETS ARE NUMBERS, NOT JUDGMENTS. `BENCH_P95_TOLERANCE_PCT` is how much
# slower the candidate may be at the 95th percentile than the daemon it
# replaces, in percent. `BENCH_RSS_CEILING_MB` is an absolute resident-set
# ceiling for the candidate. Neither has a default: a default is a number
# nobody chose, and the whole point of the row is that somebody chose it.
#
# TWO MODES. With BASE_URL alone the lane RECORDS — it measures and prints and
# gates on nothing, which is all that is possible against a deployment whose
# process this machine cannot see. With COMPARE_URL as well it COMPARES, and
# that is the mode the cutover decision reads.
#
# BENCH_HEY and BENCH_RSS_PROBE are overridable so the self-tests can drive the
# lane without a load generator or a container. Nothing else sets them.
#
# Exits 0 on success, 1 on the first failing assertion (with diagnostic).

set -euo pipefail

# Named, because a bare 20 in a recipe is a number nobody can argue with.
readonly BUDGET_TOLERANCE_NAME="BENCH_P95_TOLERANCE_PCT"
readonly BUDGET_RSS_NAME="BENCH_RSS_CEILING_MB"

# The load shape. Defaults, unlike the budgets: these describe how hard the lane
# pushes, not what verdict it reaches, so a value nobody chose is harmless here.
BENCH_ROUTE="${BENCH_ROUTE:-/healthz}"
BENCH_DURATION_SEC="${BENCH_DURATION_SEC:-20}"
BENCH_CONCURRENCY="${BENCH_CONCURRENCY:-20}"
BENCH_TIMEOUT_SEC="${BENCH_TIMEOUT_SEC:-5}"
readonly BENCH_ROUTE BENCH_DURATION_SEC BENCH_CONCURRENCY BENCH_TIMEOUT_SEC

# The container whose resident set is the candidate's. Unset means unmeasurable
# — a remote deployment — and the lane says so rather than inventing a number.
BENCH_RSS_CONTAINER="${BENCH_RSS_CONTAINER:-}"
# Substituted by the self-tests. In a real run these are the real tools.
BENCH_HEY="${BENCH_HEY:-hey}"
BENCH_RSS_PROBE="${BENCH_RSS_PROBE:-}"
readonly BENCH_RSS_CONTAINER BENCH_HEY BENCH_RSS_PROBE

FAIL=0
err() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()  { printf "OK:   %s\n" "$*"; }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# A budget is present and is a number. Both halves matter: an empty value and a
# typo'd one fail the same way for the caller, and neither may reach a
# comparison, where an empty operand would silently read as zero.
require_budget() {
  local name="$1" value="$2"
  if [ -z "$value" ]; then
    err "$name is unset — a benchmark lane with no budget grades nothing, which is not a pass"
    return 1
  fi
  case "$value" in
    '' | *[!0-9.]* | *.*.*)
      err "$name is '$value', which is not a number"
      return 1
      ;;
  esac
  ok "$name = $value"
}

# `hey` against one base URL, reduced to the p95 in milliseconds.
#
# The percentile is computed from the raw sample column rather than read from
# hey's own summary block, because that block's wording has changed between
# releases and a parse that silently matches nothing yields an empty string —
# which every numeric comparison below would read as zero, and zero passes
# every budget.
measure_p95_ms() {
  local base="$1" samples="$WORK_DIR/samples.csv" total
  "$BENCH_HEY" -m GET -z "${BENCH_DURATION_SEC}s" -c "$BENCH_CONCURRENCY" \
    -t "$BENCH_TIMEOUT_SEC" -o csv "$base$BENCH_ROUTE" >"$samples" 2>/dev/null || {
    err "the load generator exited non-zero against $base"
    return 1
  }
  total="$(tail -n +2 "$samples" | wc -l | tr -d ' ')"
  if [ "${total:-0}" -eq 0 ]; then
    err "the load generator produced zero samples against $base — nothing was measured"
    return 1
  fi
  tail -n +2 "$samples" | awk -F, '{print $1}' | sort -n \
    | awk -v total="$total" 'NR == int(total * 0.95) { printf "%.2f", $1 * 1000; exit }'
}

# The candidate's resident set, in whole megabytes, or empty when this machine
# cannot see the process. `docker stats` reports "123.4MiB / 7.654GiB"; the
# figure before the slash is the one that is bounded.
measure_rss_mb() {
  if [ -n "$BENCH_RSS_PROBE" ]; then
    "$BENCH_RSS_PROBE"
    return
  fi
  [ -n "$BENCH_RSS_CONTAINER" ] || return 0
  docker stats --no-stream --format '{{.MemUsage}}' "$BENCH_RSS_CONTAINER" 2>/dev/null \
    | awk '{ split($1, parts, /[A-Za-z]/); unit = $1; sub(/^[0-9.]+/, "", unit);
             value = parts[1] + 0
             if (unit ~ /^GiB/) value *= 1024
             else if (unit ~ /^KiB/) value /= 1024
             printf "%.0f", value }'
}

# a is within tolerance percent of b. Kept as its own function because it is the
# verdict — the one line that decides whether the swap is allowed — and a
# verdict inlined in a pipeline is a verdict nobody tests.
within_tolerance() {
  local candidate="$1" baseline="$2" tolerance_pct="$3"
  awk -v c="$candidate" -v b="$baseline" -v t="$tolerance_pct" \
    'BEGIN { exit !(c <= b * (1 + t / 100)) }'
}

# RECORD — one daemon, measured and printed, gating on nothing.
#
# It gates on nothing because there is nothing to gate against: a percentile
# has no verdict without the number it is allowed to be worse than. The budgets
# are still required, so the command that records and the command that decides
# cannot drift into being configured differently.
record_mode() {
  local base="$1" p95 rss
  p95="$(measure_p95_ms "$base")" || return 1
  rss="$(measure_rss_mb)"
  printf 'baseline p95_ms=%s route=%s duration_s=%s concurrency=%s\n' \
    "$p95" "$BENCH_ROUTE" "$BENCH_DURATION_SEC" "$BENCH_CONCURRENCY"
  if [ -n "$rss" ]; then
    printf 'baseline rss_mb=%s container=%s\n' "$rss" "$BENCH_RSS_CONTAINER"
  else
    printf 'baseline rss_mb=unmeasurable — no container named, so the process is not visible from here\n'
  fi
  ok "recorded $base — no verdict reached, and none claimed"
}

# COMPARE — the mode the cutover decision reads.
compare_mode() {
  local baseline_url="$1" candidate_url="$2"
  local baseline_p95 candidate_p95 rss
  baseline_p95="$(measure_p95_ms "$baseline_url")" || return 1
  candidate_p95="$(measure_p95_ms "$candidate_url")" || return 1
  printf 'baseline  p95_ms=%s  %s\n' "$baseline_p95" "$baseline_url"
  printf 'candidate p95_ms=%s  %s\n' "$candidate_p95" "$candidate_url"
  if within_tolerance "$candidate_p95" "$baseline_p95" "$BENCH_P95_TOLERANCE_PCT"; then
    ok "p95 ${candidate_p95}ms is within $BUDGET_TOLERANCE_NAME=$BENCH_P95_TOLERANCE_PCT% of ${baseline_p95}ms"
  else
    err "p95 ${candidate_p95}ms exceeds ${baseline_p95}ms by more than $BUDGET_TOLERANCE_NAME=$BENCH_P95_TOLERANCE_PCT%"
  fi

  rss="$(measure_rss_mb)"
  if [ -z "$rss" ]; then
    err "$BUDGET_RSS_NAME=$BENCH_RSS_CEILING_MB is declared but no resident set could be read — set BENCH_RSS_CONTAINER, or the ceiling grades nothing"
  elif awk -v r="$rss" -v c="$BENCH_RSS_CEILING_MB" 'BEGIN { exit !(r <= c) }'; then
    ok "resident set ${rss}MB is within $BUDGET_RSS_NAME=$BENCH_RSS_CEILING_MB"
  else
    err "resident set ${rss}MB exceeds $BUDGET_RSS_NAME=$BENCH_RSS_CEILING_MB"
  fi
}

main() {
  local base="${BASE_URL:-}" compare="${COMPARE_URL:-}"

  # The budgets are checked BEFORE anything else, including BASE_URL, so a lane
  # invoked with no budgets says so rather than spending a minute under load
  # first and only then admitting it had nothing to compare against.
  require_budget "$BUDGET_TOLERANCE_NAME" "${BENCH_P95_TOLERANCE_PCT:-}" || true
  require_budget "$BUDGET_RSS_NAME" "${BENCH_RSS_CEILING_MB:-}" || true
  [ "$FAIL" -eq 0 ] || exit 1

  if [ -z "$base" ]; then
    printf "FAIL: BASE_URL is unset — the lane has nothing to measure\n" >&2
    exit 1
  fi
  command -v "$BENCH_HEY" >/dev/null 2>&1 || {
    printf "FAIL: the load generator '%s' is not on PATH. Install via: mise use -g 'ubi:rakyll/hey@latest'\n" \
      "$BENCH_HEY" >&2
    exit 1
  }

  if [ -n "$compare" ]; then
    compare_mode "${base%/}" "${compare%/}"
  else
    record_mode "${base%/}"
  fi
  exit "$FAIL"
}

main "$@"
