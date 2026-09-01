#!/usr/bin/env bash
# Fixtures and the runner every parity-lane self-test drives the lane through.
#
# Sourced by `parity_lane_test.sh`, never run on its own. The split is along
# what each half IS: this file is the contract document, the scriptable
# responder and the invocation, and that one is the assertions — so a fixture
# growing a field and a case being added stop landing in the same file.
#
# `ok`/`bad` tally into `passed`/`failed` here and are read by the summary
# there, which works because a sourced file shares the caller's shell. That is
# also why `trap cleanup EXIT` set here fires for the whole run.

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
apply_seed() {
  local target_base="$1" seeded_status="$2" seeded_header="$3" seeded_detail="$4"
  [ -n "$target_base" ] && [ "$base" = "$target_base" ] \
    && [ "$seed_route" = "$method $path" ] || return
  [ -n "$seeded_status" ] && status="$seeded_status"
  [ -n "$seeded_header" ] && extra_header="$seeded_header"
  [ -n "$seeded_detail" ] && detail="$seeded_detail"
}
apply_seed "$seed_base" "${SEED_STATUS:-}" "${SEED_HEADER:-}" "${SEED_DETAIL:-}"
apply_seed "${SECOND_SEED_BASE:-}" "${SECOND_SEED_STATUS:-}" \
  "${SECOND_SEED_HEADER:-}" "${SECOND_SEED_DETAIL:-}"
# What a router answers for a path it does not have: the bare status, no
# envelope and no body. The unmatched-route probe always gets this, and so does
# any seeded 404 unless the test is deliberately staging an arbitrary one.
# The old fixture answered the SAME headers and body whatever the status, which
# is what let a predicate demanding snapshot equality look correct.
case "$path" in
  */parity-lane-absence-probe/*) status="404"; detail="" ;;
esac
if [ "$status" = "404" ] && [ -z "${SEED_DETAIL:-}" ]; then
  printf '404\n\n\n'
  exit 0
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
