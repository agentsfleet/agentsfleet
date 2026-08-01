#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/test_search.sh
source "$SCRIPT_DIR/../../lib/test_search.sh"
SCRIPT_UNDER_TEST="$SCRIPT_DIR/02_service_health.sh"
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
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
printf 'op %s\n' "$*" >>"$CALLS"
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read) printf '%s\n' "${BYPASS_SECRET-bypass-secret}" ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/curl" <<'STUB'
#!/usr/bin/env bash
printf 'curl %s\n' "$*" >>"$CALLS"
url="${*: -1}"
if [ -n "${CURL_FAIL_MATCH:-}" ] && [[ "$url" == *"$CURL_FAIL_MATCH"* ]]; then
  exit 22
fi
case "$url" in
  */readyz) printf '%s\n' "${READY_PAYLOAD:-{\"ready\":true}}" ;;
  *) : ;;
esac
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl"

run_health() {
  local environment="$1"
  shift
  : >"$calls"
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    ENV="$environment" \
    ALLOW_VAULT_READS=1 \
    "$@" \
    bash "$SCRIPT_UNDER_TEST" 2>&1
}

test_should_verify_development_services() {
  local name="test_should_verify_development_services"
  local output status=0
  output="$(run_health dev)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! rg --fixed-strings --quiet \
    'https://api-dev.agentsfleet.net/healthz' "$calls" ||
    ! rg --fixed-strings --quiet \
      'https://api-dev.agentsfleet.net/readyz' "$calls" ||
    ! rg --fixed-strings --quiet \
      'https://app-dev.agentsfleet.net/sign-in' "$calls"; then
    bad "$name" "development endpoints were not all checked"
  elif ! rg --quiet -- '--config .* --fail .*app-dev.agentsfleet.net/sign-in' \
    "$calls"; then
    bad "$name" "development sign-in omitted the deployment-protection config"
  elif [[ "$output" == *bypass-secret* ]] || rg --quiet bypass-secret "$calls"; then
    bad "$name" "deployment-protection credential escaped into output or arguments"
  else
    ok "$name"
  fi
}

test_should_verify_production_services_without_bypass() {
  local name="test_should_verify_production_services_without_bypass"
  local output status=0
  output="$(run_health prod)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! rg --fixed-strings --quiet \
    'https://api.agentsfleet.net/healthz' "$calls" ||
    ! rg --fixed-strings --quiet \
      'https://api.agentsfleet.net/readyz' "$calls" ||
    ! rg --fixed-strings --quiet \
      'https://app.agentsfleet.net/sign-in' "$calls"; then
    bad "$name" "production endpoints were not all checked"
  elif rg --quiet 'vercel-bypass-app' "$calls"; then
    bad "$name" "production verification read the development bypass credential"
  elif rg --quiet -- '--config .*app.agentsfleet.net/sign-in' "$calls"; then
    bad "$name" "production sign-in used deployment protection"
  else
    ok "$name"
  fi
}

test_should_accumulate_service_failures() {
  local name="test_should_accumulate_service_failures"
  local case_entry expected output status
  local -a cases=(
    'CURL_FAIL_MATCH=healthz|https://api-dev.agentsfleet.net/healthz'
    'READY_PAYLOAD={"ready":false}|https://api-dev.agentsfleet.net/readyz'
    'CURL_FAIL_MATCH=app-dev.agentsfleet.net|https://app-dev.agentsfleet.net/sign-in'
  )

  for case_entry in "${cases[@]}"; do
    expected="${case_entry#*|}"
    status=0
    output="$(run_health dev "${case_entry%%|*}")" || status=$?
    if [ "$status" -ne 1 ] || [[ "$output" != *"FAIL: $expected"* ]] ||
      [[ "$output" != *"service check(s) failed"* ]]; then
      bad "$name" "$case_entry did not fail closed: $output"
      return
    fi
  done
  ok "$name"
}

test_should_reject_malformed_bypass_credentials() {
  local name="test_should_reject_malformed_bypass_credentials"
  local output status quote
  quote="$(printf '\042')"
  local -a credentials=('BYPASS_SECRET=' "BYPASS_SECRET=bad${quote}suffix")

  for credential in "${credentials[@]}"; do
    status=0
    output="$(run_health dev "$credential")" || status=$?
    if [ "$status" -ne 1 ] ||
      [[ "$output" != *"bypass credential is missing or malformed"* ]]; then
      bad "$name" "$credential did not fail closed: $output"
      return
    fi
  done
  ok "$name"
}

test_should_reject_production_sign_in_failure() {
  local name="test_should_reject_production_sign_in_failure"
  local output status=0
  output="$(run_health prod CURL_FAIL_MATCH=app.agentsfleet.net)" || status=$?
  if [ "$status" -ne 1 ] ||
    [[ "$output" != *"FAIL: https://app.agentsfleet.net/sign-in"* ]]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}

test_should_verify_development_services
test_should_verify_production_services_without_bypass
test_should_accumulate_service_failures
test_should_reject_malformed_bypass_credentials
test_should_reject_production_sign_in_failure

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
