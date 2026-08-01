#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/test_search.sh
source "$SCRIPT_DIR/../../lib/test_search.sh"
SCRIPT="$SCRIPT_DIR/02_vercel_env.sh"
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
      */vercel-api-token/credential) printf 'vercel-secret\n' ;;
      */posthog-*/credential) printf 'posthog-value\n' ;;
      */clerk-*/publishable-key) printf 'clerk-publishable\n' ;;
      */clerk-*/secret-key) printf 'clerk-secret\n' ;;
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
input_file=""
previous=""
for argument in "$@"; do
  case "$previous" in
    --request) method="$argument" ;;
    --data-binary) input_file="${argument#@}" ;;
  esac
  previous="$argument"
done
url="${*: -1}"

if [ "$method" = "POST" ]; then
  cp "$input_file" "$CAPTURES/$(date +%s%N).json"
  printf '{"created":true}\n'
elif [[ "$url" == */v10/projects/agentsfleet-website ]]; then
  printf '{"id":"website-id"}\n'
elif [[ "$url" == */v10/projects/agentsfleet-app ]]; then
  printf '{"id":"app-id"}\n'
elif [[ "$url" == */env?decrypt=false ]]; then
  printf '{"envs":[]}\n'
elif [[ "$url" == */v10/projects?limit=100 ]]; then
  printf '{"projects":[]}\n'
else
  printf '{}\n'
fi
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl"

run_script() {
  : >"$calls"
  rm -f "$captures"/*
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    CAPTURES="$captures" \
    ALLOW_VAULT_READS=1 \
    "$@" 2>&1
}

test_should_apply_complete_matrix_without_exposing_values() {
  local name="test_should_apply_complete_matrix_without_exposing_values"
  local output status=0
  output="$(
    run_script ALLOW_VERCEL_WRITES=1 bash "$SCRIPT" --apply
  )" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(find "$captures" -type f | wc -l | tr -d ' ')" -ne 16 ]; then
    bad "$name" "expected sixteen target updates"
  elif rg --quiet 'vercel-secret|posthog-value|clerk-secret' "$calls"; then
    bad "$name" "a credential appeared in process arguments"
  elif ! rg --quiet '"key": "NEXT_PUBLIC_API_URL"' "$captures"; then
    bad "$name" "dashboard API URL was not managed"
  elif ! rg --quiet '"key": "CLERK_SECRET_KEY"' "$captures"; then
    bad "$name" "dashboard Clerk secret was not managed"
  elif ! rg --quiet '"key": "VITE_APP_BASE_URL"' "$captures"; then
    bad "$name" "website dashboard URL was not managed"
  elif rg --quiet 'agentsfleet-agents-dev' "$calls" "$captures"; then
    bad "$name" "the static installer received unused environment values"
  else
    ok "$name"
  fi
}

test_should_require_write_approval() {
  local name="test_should_require_write_approval"
  local output status=0
  output="$(run_script bash "$SCRIPT" --apply)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "Vercel writes ran without approval"
  elif find "$captures" -type f | rg --quiet .; then
    bad "$name" "the denied apply wrote Vercel values"
  else
    ok "$name"
  fi
}

test_should_report_drift_without_writes() {
  local name="test_should_report_drift_without_writes"
  local output status=0
  output="$(run_script bash "$SCRIPT" --check)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "empty Vercel projects passed the drift check"
  elif find "$captures" -type f | rg --quiet .; then
    bad "$name" "read-only check wrote Vercel values"
  else
    ok "$name"
  fi
}

test_should_require_vault_approval() {
  local name="test_should_require_vault_approval"
  local output status=0
  output="$(
    run_script ALLOW_VAULT_READS=0 bash "$SCRIPT" --check
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "vault reads ran without approval"
  else
    ok "$name"
  fi
}

test_should_ignore_ambient_api_endpoint_override() {
  local name="test_should_ignore_ambient_api_endpoint_override"
  local output status=0
  output="$(
    run_script VERCEL_API=https://attacker.example bash "$SCRIPT" --check
  )" || status=$?
  if [ "$status" -eq 0 ] || rg --quiet attacker.example "$calls" ||
    ! rg --quiet api.vercel.com "$calls"; then
    bad "$name" "check used the ambient endpoint: $output"
    return
  fi
  status=0
  output="$(
    run_script VERCEL_API=https://attacker.example ALLOW_VERCEL_WRITES=1 \
      bash "$SCRIPT" --apply
  )" || status=$?
  if [ "$status" -ne 0 ] || rg --quiet attacker.example "$calls" ||
    ! rg --quiet api.vercel.com "$calls"; then
    bad "$name" "apply used the ambient endpoint: $output"
  else
    ok "$name"
  fi
}

test_should_apply_complete_matrix_without_exposing_values
test_should_require_write_approval
test_should_report_drift_without_writes
test_should_require_vault_approval
test_should_ignore_ambient_api_endpoint_override

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
