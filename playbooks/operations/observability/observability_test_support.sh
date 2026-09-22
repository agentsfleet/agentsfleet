#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/test_search.sh
source "$SCRIPT_DIR/../../lib/test_search.sh"
GATE="$SCRIPT_DIR/00_gate.sh"
PROVIDER_DIR="$SCRIPT_DIR"
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
captures="$work_dir/captures"
mkdir -p "$stub_dir" "$captures"
trap 'rm -rf "$work_dir"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read)
    case "${2:-}" in
      */grafana-url) printf 'https://grafana.test\n' ;;
      */grafana-sa-token) printf 'grafana-secret\n' ;;
      */grafana-namespace) printf 'default\n' ;;
      */prometheus-datasource-uid) printf 'prometheus-main\n' ;;
      */loki-datasource-uid) printf 'loki-main\n' ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/curl" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
method=GET
output_file=""
input_file=""
write_status=0
previous=""
for argument in "$@"; do
  case "$previous" in
    --request) method="$argument" ;;
    --output) output_file="$argument" ;;
    --data-binary) input_file="${argument#@}" ;;
  esac
  [ "$argument" = "--write-out" ] && write_status=1
  previous="$argument"
done
url="${*: -1}"
status=200
body='{}'

case "$url" in
  */api/datasources/uid/prometheus-main)
    body="{\"uid\":\"prometheus-main\",\"type\":\"${MOCK_PROM_TYPE:-prometheus}\"}"
    ;;
  */api/datasources/proxy/uid/prometheus-main/api/v1/query)
    body='{"status":"success","data":{"result":[{"value":[1,"0"]}]}}'
    ;;
  */api/datasources/uid/loki-main)
    body="{\"uid\":\"loki-main\",\"type\":\"${MOCK_LOKI_TYPE:-loki}\"}"
    ;;
  */api/datasources/proxy/uid/loki-main/loki/api/v1/labels)
    body='{"status":"success","data":["service_name","service_namespace"]}'
    ;;
  */folders/agentsfleet-dev | */dashboards/agentsfleet-runtime-dev | */alertrules/*-dev)
    if [ "${MOCK_MODE:-create}" = "create" ]; then
      status=404
    else
      body='{"metadata":{"resourceVersion":"7"},"spec":{}}'
    fi
    ;;
  */folders | */dashboards | */alertrules)
    status=201
    body='{"metadata":{"resourceVersion":"1"}}'
    ;;
esac

if [ "$method" != "GET" ] && [ -n "${MOCK_WRITE_STATUS:-}" ]; then
  status="$MOCK_WRITE_STATUS"
fi

if [ "$method" != "GET" ] && [ -n "$input_file" ]; then
  capture="$CAPTURES/$(basename "$input_file").$method.json"
  cp "$input_file" "$capture"
fi
if [ -n "$output_file" ]; then
  printf '%s\n' "$body" >"$output_file"
else
  printf '%s\n' "$body"
fi
if [ "$write_status" -eq 1 ]; then
  printf '%s' "$status"
fi
STUB

cat >"$stub_dir/rg" <<'STUB'
#!/usr/bin/env bash
echo "ERROR: production playbooks must not require rg" >&2
exit 127
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl" "$stub_dir/rg"

run_script() {
  : >"$calls"
  rm -f "$captures"/*
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    CAPTURES="$captures" \
    OBS_ENV=dev \
    ALLOW_VAULT_READS=1 \
    ALLOW_OBSERVABILITY_WRITES=1 \
    "$@" 2>&1
}

# Every test below runs in its own backgrounded subshell (see the runner at
# the bottom of this file), and run_script's `: >"$calls"` / `rm -f
# "$captures"/*` operate on paths, not shell state — two tests sharing the
# file-scope $calls/$captures would genuinely race: one test's assertion
# reading a curl-argument log truncated mid-read by another test's run_script
# call. Each test declares its OWN local calls/captures below. bash resolves
# a free variable inside a called function by walking UP the call stack, and
# a command-substitution subshell inherits that whole call stack at fork time
# — so run_script, invoked from inside a test function that just did `local
# calls=...`, sees THAT test's path, never the file-scope default declared at
# the top of this file. The file-scope $calls/$captures become dead once
# every test shadows them; kept only so run_script has something to name if a
# future caller forgets to.

# A mutated copy of the assets, so a negative test proves the checker rejects a
# defect without the risk of leaving the real asset broken on a failed run.
broken_assets() {
  local dir
  dir="$(mktemp -d -p "$work_dir")"
  cp "$PROVIDER_DIR/assets/dashboard.json" "$PROVIDER_DIR/assets/alerts.json" "$dir/"
  printf '%s' "$dir"
}

# Runs every name in TEST_NAMES, each in its own backgrounded subshell, and
# reports. Shared because two suites grew out of one file and a second copy of
# a parallel test runner is a second place for a race to hide.
run_suite() {
  local result_dir name
  result_dir="$(mktemp -d)"
  local pids=()
  # A declared name with no function is a FAILURE, not a pass. The tally below
  # only looks for a FAIL line, so a test deleted or renamed out from under its
  # entry logged "command not found" and counted green — which is how two
  # refusal guards reported success for an edit that had removed their bodies.
  for name in "${TEST_NAMES[@]}"; do
    if ! declare -F "$name" >/dev/null; then
      echo "FAIL $name" >"$result_dir/$name.log"
      echo "       declared in TEST_NAMES but no such function" >>"$result_dir/$name.log"
      continue
    fi
    ( "$name" ) >"$result_dir/$name.log" 2>&1 &
    pids+=("$!")
  done
  for pid in "${pids[@]}"; do wait "$pid"; done
  for name in "${TEST_NAMES[@]}"; do
    cat "$result_dir/$name.log"
    if grep -q '^FAIL ' "$result_dir/$name.log"; then
      failed=$((failed + 1))
    else
      passed=$((passed + 1))
    fi
  done
  rm -rf "$result_dir"
  printf '\n%d passed, %d failed\n' "$passed" "$failed"
  [ "$failed" -eq 0 ]
}
