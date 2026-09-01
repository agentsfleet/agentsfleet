#!/usr/bin/env bash
# Self-tests for parity_lane.sh — the differ, the normalisation, and the guard.
#
#     bash scripts/parity_lane_test.sh
#
# The harness drives fixtures through PARITY_OPENAPI and PARITY_PROBE, so every
# test here exercises the SAME roster, snapshot and diff code a live run does —
# hermetically, in about a second, which is what lets it ride `make lint-all`.
#
# What that deliberately does NOT cover is `probe_via_curl` itself: substituting
# the responder is exactly what skips it. The curl invocation is proven instead
# by `make test-parity` against the shipped image (rubric R3), which runs before
# the PR anyway. A stub server here would move that proof a few hours earlier at
# the cost of a python3 dependency inside a shell self-test; not worth it.
#
# Covers Dimension 4.1's `test_parity_lane_detects_difference`: the status,
# header and body cases are the three ways two daemons differ, and the
# volatile-only case is what stops the lane being red on every run.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly LANE="$SCRIPT_DIR/parity_lane.sh"

# The fixtures, the responder and the runner, sourced rather than spelled here:
# the file was over the length cap and the harness is its own concern.
# shellcheck source=scripts/parity_lane_test_harness.sh
. "$SCRIPT_DIR/parity_lane_test_harness.sh"


CONTRACT="$WORK_DIR/openapi.json"
EMPTY_CONTRACT="$WORK_DIR/openapi-empty.json"
write_responder
write_contract "$CONTRACT"
write_contract "$EMPTY_CONTRACT" empty

# --------------------------------------------------------------------------
# test_parity_lane_detects_difference — identical daemons diff empty.
#
# It runs FIRST because every negative case below is only meaningful if the
# baseline is green: a lane that failed on everything would "detect" every
# seeded difference while proving nothing.
# --------------------------------------------------------------------------
status="$(run_lane "$CONTRACT" BASE_URL="$BASE_A" COMPARE_URL="$BASE_B")"
if [ "$status" = "0" ]; then
  ok "identical daemons diff empty"
else
  bad "identical daemons diff empty" "expected exit 0, got $status: $(lane_output)"
fi

# Volatile-only difference. The responder mints a different date, x-request-id
# and body request_id on every call and per base, so this case is ALREADY
# covered by the run above — asserted separately because it is the property
# that decides whether the lane is usable at all, and a future normalisation
# regression must name itself rather than hide inside the baseline.
if [ "$status" = "0" ] && printf '%s' "$(lane_output)" | grep -q 'identical'; then
  ok "per-request volatile fields normalise away"
else
  bad "per-request volatile fields normalise away" "baseline was not clean: $(lane_output)"
fi

# --------------------------------------------------------------------------
# A seeded difference fails, naming route and method — the three shapes.
#
# The seed matches on the CONCRETE path the lane probes and the assertion reads
# the TEMPLATE path the lane reports, so these cases also prove the path
# parameter was substituted: a lane that skipped substitution would never
# trigger the seed and the case would fail rather than pass quietly.
# --------------------------------------------------------------------------
PATH_PARAM_PLACEHOLDER="$(sed -n 's/^readonly PATH_PARAM_PLACEHOLDER="\(.*\)"$/\1/p' "$LANE")"
readonly PATH_PARAM_PLACEHOLDER
[ -n "$PATH_PARAM_PLACEHOLDER" ] || {
  printf 'FAIL could not read PATH_PARAM_PLACEHOLDER from %s\n' "$LANE" >&2
  exit 1
}

concrete() { printf '%s' "${1//\{workspace_id\}/$PATH_PARAM_PLACEHOLDER}"; }

seeded_case() {
  local label="$1" method="$2" template="$3" kind="$4"
  local seed_status="" seed_header="" seed_detail=""
  case "$kind" in
    status) seed_status="403" ;;
    header) seed_header="cache-control: max-age=60" ;;
    body)   seed_detail="A different sentence entirely" ;;
  esac
  local exit_status
  exit_status="$(run_lane "$CONTRACT" BASE_URL="$BASE_A" COMPARE_URL="$BASE_B" \
    SEED_ROUTE="$method $(concrete "$template")" SEED_STATUS="$seed_status" \
    SEED_HEADER="$seed_header" SEED_DETAIL="$seed_detail")"
  if [ "$exit_status" = "0" ]; then
    bad "$label" "expected a non-zero exit, got 0: $(lane_output)"
  elif ! lane_output | grep -qF -- "$method $template"; then
    bad "$label" "failed without naming '$method $template': $(lane_output)"
  else
    ok "$label"
  fi
}

seeded_case "a seeded status difference fails naming route and method" \
  GET "$FIXTURE_ROUTE_PLAIN" status
seeded_case "a seeded header difference fails naming route and method" \
  POST "$FIXTURE_ROUTE_PARAM" header
seeded_case "a seeded body difference fails naming route and method" \
  GET "$FIXTURE_ROUTE_PARAM" body

# --------------------------------------------------------------------------
# RECORD mode — one daemon, and the two ways it fails.
# --------------------------------------------------------------------------
status="$(run_lane "$CONTRACT" BASE_URL="$BASE_A")"
if [ "$status" = "0" ]; then
  ok "record mode passes when every contract route answers"
else
  bad "record mode passes when every contract route answers" "exit $status: $(lane_output)"
fi

# A route the daemon does not mount. Seeded on base A because record mode
# probes only the one base.
status="$(run_lane "$CONTRACT" BASE_URL="$BASE_A" SEED_BASE="$BASE_A" \
  SEED_ROUTE="GET $FIXTURE_ROUTE_PLAIN" SEED_STATUS="404")"
if [ "$status" != "0" ] && lane_output | grep -qF "GET $FIXTURE_ROUTE_PLAIN"; then
  ok "record mode fails a route that answers 404 — the route is not mounted"
else
  bad "record mode fails a route that answers 404" "exit $status: $(lane_output)"
fi

status="$(run_lane "$CONTRACT" BASE_URL="$BASE_A" SEED_BASE="$BASE_A" \
  SEED_ROUTE="GET $FIXTURE_ROUTE_PLAIN" SEED_STATUS="000")"
if [ "$status" != "0" ] && lane_output | grep -qF "no answer"; then
  ok "record mode fails a route that never answered"
else
  bad "record mode fails a route that never answered" "exit $status: $(lane_output)"
fi

# --------------------------------------------------------------------------
# The guard and the refusals.
# --------------------------------------------------------------------------
status="$(run_lane "$EMPTY_CONTRACT" BASE_URL="$BASE_A" COMPARE_URL="$BASE_B")"
if [ "$status" != "0" ] && lane_output | grep -qF "probed nothing"; then
  ok "an empty roster fails — a lane that graded nothing is not a pass"
else
  bad "an empty roster fails" "exit $status: $(lane_output)"
fi

status="$(run_lane "$CONTRACT")"
if [ "$status" != "0" ] && lane_output | grep -qF "BASE_URL is unset"; then
  ok "an unset BASE_URL fails naming the variable"
else
  bad "an unset BASE_URL fails naming the variable" "exit $status: $(lane_output)"
fi

env PARITY_OPENAPI="$WORK_DIR/absent.json" BASE_URL="$BASE_A" \
  bash "$LANE" >"$WORK_DIR/out" 2>&1
status="$?"
if [ "$status" != "0" ] && lane_output | grep -qF "no contract at"; then
  ok "a missing contract document fails naming the path"
else
  bad "a missing contract document fails naming the path" "exit $status: $(lane_output)"
fi

# --------------------------------------------------------------------------
# The declared-divergence register — the seam between this lane and the cutover
# runbook. Without it the lane fails forever on a route the daemon deliberately
# does not serve, and the only remaining fixes are to un-declare the decision or
# to stop running the lane.
# --------------------------------------------------------------------------
REGISTER="$WORK_DIR/register.md"
cat >"$REGISTER" <<REG
| # | Divergence | Declared by | Why |
|---|---|---|---|
| D1 | \`GET $FIXTURE_ROUTE_PLAIN\` is declared and NOT served. | a fixture | because the test says so |
REG

declared_compare_case() {
  local label="$1" expectation="$2" diagnostic="$3"
  shift 3
  local exit_status
  exit_status="$(run_lane "$CONTRACT" PARITY_REGISTER="$REGISTER" \
    SEED_ROUTE="GET $FIXTURE_ROUTE_PLAIN" BASE_URL="$BASE_A" \
    COMPARE_URL="$BASE_B" "$@")"
  if [ "$expectation" = "pass" ] \
    && [ "$exit_status" = "0" ] && lane_output | grep -qF "declared:"; then
    ok "$label"
  elif [ "$expectation" = "fail" ] \
    && [ "$exit_status" != "0" ] && lane_output | grep -qF "$diagnostic"; then
    ok "$label"
  else
    bad "$label" "expected $expectation with '$diagnostic', exit $exit_status: $(lane_output)"
  fi
}

status="$(env PARITY_OPENAPI="$CONTRACT" PARITY_PROBE="$WORK_DIR/responder.sh" \
  PARITY_REGISTER="$REGISTER" SEED_BASE="$BASE_A" \
  SEED_ROUTE="GET $FIXTURE_ROUTE_PLAIN" SEED_STATUS="404" \
  BASE_URL="$BASE_A" bash "$LANE" >"$WORK_DIR/out" 2>&1; printf '%s' "$?")"
if [ "$status" = "0" ] && lane_output | grep -qF "declared:"; then
  ok "a 404 named in the divergence register is honoured, not failed"
else
  bad "a 404 named in the divergence register is honoured" "exit $status: $(lane_output)"
fi

# The register is not a blanket exemption: a DIFFERENT route answering 404 with
# the same register in place still fails, or the register would be a mute button.
status="$(env PARITY_OPENAPI="$CONTRACT" PARITY_PROBE="$WORK_DIR/responder.sh" \
  PARITY_REGISTER="$REGISTER" SEED_BASE="$BASE_A" \
  SEED_ROUTE="POST $(concrete "$FIXTURE_ROUTE_PARAM")" SEED_STATUS="404" \
  BASE_URL="$BASE_A" bash "$LANE" >"$WORK_DIR/out" 2>&1; printf '%s' "$?")"
if [ "$status" != "0" ] && lane_output | grep -qF "not mounted"; then
  ok "a 404 NOT in the register still fails — the register is not a mute button"
else
  bad "a 404 not in the register still fails" "exit $status: $(lane_output)"
fi

# A mounted route may legitimately answer 404, and reading the status alone
# called three of them missing. An OPEN route has no auth to refuse, so its
# handler runs against the placeholder segment, finds nothing, and answers a
# correct not-found with its own envelope. That is a mounted route, and RECORD
# mode must say so. The router's own unmatched-route answer is what absence
# looks like, and the seeded envelope here is deliberately not it.
status="$(env PARITY_OPENAPI="$CONTRACT" PARITY_PROBE="$WORK_DIR/responder.sh" \
  PARITY_REGISTER="$REGISTER" SEED_BASE="$BASE_A" \
  SEED_ROUTE="POST $(concrete "$FIXTURE_ROUTE_PARAM")" SEED_STATUS="404" \
  SEED_DETAIL="No such session" \
  BASE_URL="$BASE_A" bash "$LANE" >"$WORK_DIR/out" 2>&1; printf '%s' "$?")"
if [ "$status" = "0" ] && ! lane_output | grep -qF "not mounted"; then
  ok "a handler's own 404 is a mounted route, not a missing one"
else
  bad "a handler's own 404 is a mounted route" "exit $status: $(lane_output)"
fi

# COMPARE mode must read the register too. It did not: RECORD consulted it and
# COMPARE diffed every route blind, so a difference the register DECLARES — the
# case the register exists for, one daemon deliberately not serving what the
# other does — failed the lane as a regression. A reviewer caught it; the
# RECORD-mode tests above passed throughout, which is exactly why a mode
# without its own test is a mode without a guarantee.
declared_compare_case \
  "COMPARE honours a clean declared route absence" pass "declared:" \
  SEED_BASE="$BASE_A" SEED_STATUS="404"

# A refusal or timeout is not a route-absence response. Status-only checking
# used to accept this pairing because one side happened to say 404.
declared_compare_case \
  "a declared 404 does not hide a daemon that never answered" fail "disagree" \
  SEED_BASE="$BASE_A" SEED_STATUS="404" \
  SECOND_SEED_BASE="$BASE_B" SECOND_SEED_STATUS="000"

# The absent side must look like THIS daemon's unmatched-route answer. A 404 a
# handler authored — carrying an envelope and a body — is not a route absence,
# and the register does not declare it.
declared_compare_case \
  "a declared 404 does not hide an arbitrary absence body" fail "disagree" \
  SEED_BASE="$BASE_A" SEED_STATUS="404" SEED_DETAIL="Not a route absence"

# And the register is no more a mute button here than in RECORD mode: a route
# it does NOT name still fails when the two daemons disagree.
status="$(env PARITY_OPENAPI="$CONTRACT" PARITY_PROBE="$WORK_DIR/responder.sh" \
  PARITY_REGISTER="$REGISTER" SEED_BASE="$BASE_A" \
  SEED_ROUTE="POST $(concrete "$FIXTURE_ROUTE_PARAM")" SEED_STATUS="403" \
  BASE_URL="$BASE_A" COMPARE_URL="$BASE_B" bash "$LANE" >"$WORK_DIR/out" 2>&1; printf '%s' "$?")"
if [ "$status" != "0" ] && lane_output | grep -qF "disagree"; then
  ok "COMPARE still fails an UNdeclared difference"
else
  bad "COMPARE still fails an undeclared difference" "exit $status: $(lane_output)"
fi

# A declared route is PROBED, not skipped. The first cut of the COMPARE fix
# used `continue`, which accepted every difference on a declared route rather
# than the one the register names — the register working as a mute button, the
# exact failure its own RECORD-mode test above exists to prevent. Here the two
# daemons BOTH serve the declared route and disagree on status: no side is
# absent, so the declared allowance does not apply and the lane must fail.
declared_compare_case \
  "a declared route still fails on drift that is NOT the declared absence" fail "disagree" \
  SEED_BASE="$BASE_A" SEED_STATUS="500"

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ] || exit 1
[ "$passed" -gt 0 ] || { printf 'FAIL the self-test suite ran nothing\n' >&2; exit 1; }
