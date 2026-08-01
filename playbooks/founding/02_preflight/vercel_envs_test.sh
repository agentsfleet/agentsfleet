#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/03_vercel_envs.sh"
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
trap 'rm -rf -- "$work_dir"' EXIT

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
if [ "${1:-}" = "read" ]; then
  printf '%s\n' "${MOCK_TOKEN:-vercel-secret}"
  exit 0
fi
exit 1
STUB

cat >"$stub_dir/curl" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
url="${*: -1}"
if [ -n "${FAIL_PROJECT:-}" ] && [[ "$url" == */v10/projects/"$FAIL_PROJECT" ]]; then
  exit 22
fi
case "$url" in
  */v10/projects?limit=100)
    printf '{"projects":[{"name":"agentsfleet-website"},{"name":"agentsfleet-app"}]}\n'
    ;;
  */v10/projects/agentsfleet-website | */v10/projects/agentsfleet-app)
    printf '{"id":"project-id"}\n'
    ;;
  */v9/projects/agentsfleet-website/env?decrypt=false)
    payload='{"envs":[{"key":"VITE_APP_BASE_URL","target":["production","preview"]},{"key":"VITE_POSTHOG_KEY","target":["production","preview"]},{"key":"VITE_POSTHOG_HOST","target":["production","preview"]}]}'
    ;;
  */v9/projects/agentsfleet-app/env?decrypt=false)
    payload='{"envs":[{"key":"NEXT_PUBLIC_API_URL","target":["production","preview"]},{"key":"NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY","target":["production","preview"]},{"key":"CLERK_SECRET_KEY","target":["production","preview"]},{"key":"NEXT_PUBLIC_POSTHOG_KEY","target":["production","preview"]},{"key":"NEXT_PUBLIC_POSTHOG_HOST","target":["production","preview"]}]}'
    ;;
  *)
    exit 22
    ;;
esac
if [ -n "${payload:-}" ] && [ -n "${MISSING_KEY:-}" ]; then
  jq --arg key "$MISSING_KEY" '.envs |= map(select(.key != $key))' <<<"$payload"
elif [ -n "${payload:-}" ]; then
  printf '%s\n' "$payload"
fi
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl"

run_script() {
  : >"$calls"
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    ENV=all \
    OP_READ_RETRIES=1 \
    OP_READ_MIN_INTERVAL_SECONDS=0 \
    "$@" 2>&1
}

test_should_accept_complete_project_target_inventory() {
  local name="test_should_accept_complete_project_target_inventory"
  local output status=0
  output="$(run_script bash "$SCRIPT")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif rg --quiet 'vercel-secret' "$calls" <<<"$output"; then
    bad "$name" "Vercel token appeared in output or process arguments"
  else
    ok "$name"
  fi
}

test_should_reject_missing_target_inventory() {
  local name="test_should_reject_missing_target_inventory"
  local output status=0
  output="$(run_script MISSING_KEY=NEXT_PUBLIC_API_URL bash "$SCRIPT")" || status=$?
  if [ "$status" -ne 1 ] || [[ "$output" != *"MISSING: Vercel agentsfleet-app / NEXT_PUBLIC_API_URL"* ]]; then
    bad "$name" "missing variable returned $status: $output"
  else
    ok "$name"
  fi
}

test_should_reject_missing_project() {
  local name="test_should_reject_missing_project"
  local output status=0
  output="$(run_script FAIL_PROJECT=agentsfleet-app bash "$SCRIPT")" || status=$?
  if [ "$status" -ne 1 ] || [[ "$output" != *"Vercel project not found: agentsfleet-app"* ]]; then
    bad "$name" "missing project returned $status: $output"
  else
    ok "$name"
  fi
}

test_should_reject_malformed_token_before_network_access() {
  local name="test_should_reject_malformed_token_before_network_access"
  local output status=0
  output="$(run_script 'MOCK_TOKEN=bad"token' bash "$SCRIPT")" || status=$?
  if [ "$status" -ne 1 ] || [ -s "$calls" ]; then
    bad "$name" "malformed token returned $status or reached curl: $output"
  else
    ok "$name"
  fi
}

test_should_ignore_ambient_api_endpoint_override() {
  local name="test_should_ignore_ambient_api_endpoint_override"
  local output status=0
  output="$(
    run_script VERCEL_API=https://attacker.example bash "$SCRIPT"
  )" || status=$?
  if [ "$status" -ne 0 ] || rg --quiet attacker.example "$calls" ||
    ! rg --quiet api.vercel.com "$calls"; then
    bad "$name" "preflight used the ambient endpoint: $output"
  else
    ok "$name"
  fi
}

test_should_accept_complete_project_target_inventory
test_should_reject_missing_target_inventory
test_should_reject_missing_project
test_should_reject_malformed_token_before_network_access
test_should_ignore_ambient_api_endpoint_override

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
