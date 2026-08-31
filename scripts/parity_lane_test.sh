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

# The fixture roster: one route with a path parameter, one without, so the
# placeholder substitution is on the path every test takes.
readonly FIXTURE_ROUTE_PLAIN="/healthz"
readonly FIXTURE_ROUTE_PARAM="/v1/workspaces/{workspace_id}/fleets"
readonly BASE_A="http://parity-a.invalid"
readonly BASE_B="http://parity-b.invalid"

passed=0
failed=0
ok()  { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

WORK_DIR="$(mktemp -d)"
readonly WORK_DIR
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

# An OpenAPI document with exactly the two fixture routes, or an empty one.
write_contract() {
  local path="$1" empty="${2:-}"
  if [ -n "$empty" ]; then
    printf '{"paths":{}}\n' >"$path"
    return
  fi
  jq -n --arg plain "$FIXTURE_ROUTE_PLAIN" --arg param "$FIXTURE_ROUTE_PARAM" \
    '{paths: {($plain): {get: {}}, ($param): {get: {}, post: {}}}}' >"$path"
}

# A responder taking <base> <method> <path>. The SEED_* variables select what
# base B changes, so a test names ONE difference and the rest of the corpus
# stays identical — otherwise a passing diff would prove nothing.
write_responder() {
  cat >"$WORK_DIR/responder.sh" <<'RESPONDER'
#!/usr/bin/env bash
set -uo pipefail
base="$1"; method="$2"; path="$3"
seed_base="${SEED_BASE:-}"; seed_route="${SEED_ROUTE:-}"
status="401"; extra_header="cache-control: no-store"; detail="Credentials required"
if [ "$base" = "$seed_base" ] && [ "$seed_route" = "$method $path" ]; then
  [ -n "${SEED_STATUS:-}" ] && status="$SEED_STATUS"
  [ -n "${SEED_HEADER:-}" ] && extra_header="$SEED_HEADER"
  [ -n "${SEED_DETAIL:-}" ] && detail="$SEED_DETAIL"
fi
printf '%s\n' "$status"
printf 'content-type: application/problem+json\n'
printf '%s\n' "$extra_header"
# Volatile on every request AND different between the two bases on purpose:
# normalisation is the thing that has to make these compare equal.
printf 'date: %s\n' "$(date -u +%s)-$base"
printf 'x-request-id: %s\n' "req-${RANDOM}-$base"
printf '\n'
jq -nc --arg d "$detail" --arg r "rid-${RANDOM}-$base" \
  '{title: "Unauthorized", status: 401, detail: $d, request_id: $r}'
RESPONDER
  chmod +x "$WORK_DIR/responder.sh"
}

# Runs the lane against the fixtures. First argument is the contract path;
# every remaining argument is a VAR=value the run is given. `env` rather than an
# assignment prefix, because a prefix is parsed before "$@" expands.
run_lane() {
  local contract="$1"; shift
  env PARITY_OPENAPI="$contract" PARITY_PROBE="$WORK_DIR/responder.sh" \
    SEED_BASE="$BASE_B" "$@" bash "$LANE" >"$WORK_DIR/out" 2>&1
  printf '%s' "$?"
}

# The lane's own output, for a test that asserts on what it said.
lane_output() { cat "$WORK_DIR/out"; }

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

# COMPARE mode must read the register too. It did not: RECORD consulted it and
# COMPARE diffed every route blind, so a difference the register DECLARES — the
# case the register exists for, one daemon deliberately not serving what the
# other does — failed the lane as a regression. A reviewer caught it; the
# RECORD-mode tests above passed throughout, which is exactly why a mode
# without its own test is a mode without a guarantee.
status="$(env PARITY_OPENAPI="$CONTRACT" PARITY_PROBE="$WORK_DIR/responder.sh" \
  PARITY_REGISTER="$REGISTER" SEED_BASE="$BASE_A" \
  SEED_ROUTE="GET $FIXTURE_ROUTE_PLAIN" SEED_STATUS="404" \
  BASE_URL="$BASE_A" COMPARE_URL="$BASE_B" bash "$LANE" >"$WORK_DIR/out" 2>&1; printf '%s' "$?")"
if [ "$status" = "0" ] && lane_output | grep -qF "declared:"; then
  ok "COMPARE honours the register — a declared difference is not a regression"
else
  bad "COMPARE honours the register" "exit $status: $(lane_output)"
fi

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

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ] || exit 1
[ "$passed" -gt 0 ] || { printf 'FAIL the self-test suite ran nothing\n' >&2; exit 1; }
