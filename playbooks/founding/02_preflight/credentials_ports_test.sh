#!/usr/bin/env bash
# The deployment gate refuses a Postgres string on the wrong port.
#
# Split from credentials_test.sh, which is at the file cap: these cases share
# its stub shape but test one clause, and they need a stub that can put each
# string on a port of the test's choosing.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
script_under_test="$script_dir/02_credentials.sh"

passed=0
failed=0

ok() {
  passed=$((passed + 1))
  echo "  ✓ $1"
}

bad() {
  failed=$((failed + 1))
  echo "  ✗ $1: $2" >&2
}

work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
mkdir -p "$stub_dir"

cleanup() {
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

# Every ref answers; the two Postgres strings answer on the ports the test
# chose, with the password carried so a leak would be visible in the output.
cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
ref="${2:-}"
case "$ref" in
  */issuer|*/grafana-url|*/qstash/url|*/discord-*-webhook/credential)
    printf 'https://provider.example.test\n'
    ;;
  */migrator-connection-string)
    printf 'postgres://migrator:%s@db.example.test:%s/agentsfleet?sslmode=verify-full\n' \
      "$SECRET_SENTINEL" "$MIGRATOR_PORT"
    ;;
  */api-connection-string)
    printf 'postgres://api:%s@db.example.test:%s/agentsfleet?sslmode=verify-full\n' \
      "$SECRET_SENTINEL" "$API_PORT"
    ;;
  *)
    printf 'stub-value\n'
    ;;
esac
STUB
chmod +x "$stub_dir/op"

run_gate() {
  local api_port="$1"
  local migrator_port="$2"
  env \
    PATH="$stub_dir:$PATH" \
    ENV=dev \
    STAGE=deployment \
    OP_READ_RETRIES=1 \
    OP_READ_MIN_INTERVAL_SECONDS=0 \
    API_PORT="$api_port" \
    MIGRATOR_PORT="$migrator_port" \
    SECRET_SENTINEL=do-not-print-provider-secret \
    bash "$script_under_test" 2>&1
}

test_the_right_ports_pass() {
  local name="api on 6432 and migrator on 5432 pass"
  local output status=0
  output="$(run_gate 6432 5432)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "gate failed: $output"
  elif [[ "$output" != *"✓ port 6432: dev postgres api"* ]] ||
       [[ "$output" != *"✓ port 5432: dev postgres migrator"* ]]; then
    bad "$name" "port checks did not report: $output"
  else
    ok "$name"
  fi
}

test_the_api_string_off_the_pooler_port_fails() {
  local name="api on 5432 is refused"
  local output status=0
  output="$(run_gate 5432 5432)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "gate passed an API string on the direct port"
  elif [[ "$output" != *"dev postgres api (PgBouncer) must name port 6432 (found: 5432)"* ]]; then
    bad "$name" "wrong refusal: $output"
  else
    ok "$name"
  fi
}

test_the_migrator_string_on_the_pooler_port_fails() {
  local name="migrator on 6432 is refused"
  local output status=0
  output="$(run_gate 6432 6432)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "gate passed a migrator string on the pooler port"
  elif [[ "$output" != *"dev postgres migrator (direct) must name port 5432 (found: 6432)"* ]]; then
    bad "$name" "wrong refusal: $output"
  else
    ok "$name"
  fi
}

test_no_secret_is_printed_on_refusal() {
  local name="a refused string never prints its password"
  local output
  output="$(run_gate 5432 6432 || true)"
  if [[ "$output" == *"do-not-print-provider-secret"* ]]; then
    bad "$name" "the password reached the output"
  else
    ok "$name"
  fi
}

echo "credential-port regression tests"
test_the_right_ports_pass
test_the_api_string_off_the_pooler_port_fails
test_the_migrator_string_on_the_pooler_port_fails
test_no_secret_is_printed_on_refusal

echo ""
echo "results: $passed passed, $failed failed"
test "$failed" -eq 0
