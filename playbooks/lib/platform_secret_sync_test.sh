#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
script_under_test="$script_dir/platform_secret_sync.sh"

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

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
if [ "${1:-}" = "whoami" ]; then
  exit 0
fi
ref="${2:-}"
if [ -n "${EMPTY_REF_MATCH:-}" ] && [[ "$ref" == *"$EMPTY_REF_MATCH"* ]]; then
  exit 0
fi
case "$ref" in
  */agentsfleet-admin/api-key)
    printf '%s\n' "${OP_API_KEY:-agt_t$(printf 'a%.0s' {1..64})}"
    ;;
  */agentsfleet-admin/platform_admin_workspace_id)
    printf '%s\n' "${OP_WORKSPACE_ID:-0190f5a2-4b2d-7c11-8d5e-2a5f31d98210}"
    ;;
  */qstash/url)
    printf 'https://qstash.example.test\n'
    ;;
  *)
    printf 'provider-secret-sentinel\n'
    ;;
esac
STUB

cat >"$stub_dir/curl" <<'STUB'
#!/usr/bin/env bash
output=""
method=GET
payload=""
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      output="$2"
      shift 2
      ;;
    --request)
      method="$2"
      shift 2
      ;;
    --data-binary)
      payload="${2#@}"
      shift 2
      ;;
    --config)
      shift 2
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done

if [ "$method" = "GET" ]; then
  if [ "${CURL_FAIL:-0}" = "1" ]; then
    exit 22
  fi
  if [ "${SECRET_EXISTS:-0}" = "1" ]; then
    printf '{"secrets":[{"name":"%s"}]}\n' "$EXPECTED_NAME" >"$output"
  else
    printf '{"secrets":[]}\n' >"$output"
  fi
  exit 0
fi

case "$EXPECTED_NAME" in
  github-app)
    jq -e '
      (.name == "github-app" or .data != null)
      and ((.data // {}) | keys == [
        "app_id", "app_slug", "client_id", "client_secret",
        "private_key_pem", "webhook_secret"
      ])
    ' "$payload" >/dev/null
    ;;
  qstash)
    jq -e '
      (.name == "qstash" or .data != null)
      and ((.data // {}) | keys == [
        "current_signing_key", "next_signing_key", "token", "url"
      ])
    ' "$payload" >/dev/null
    ;;
  slack-app)
    jq -e '
      (.name == "slack-app" or .data != null)
      and ((.data // {}) | keys == [
        "client_id", "client_secret", "signing_secret"
      ])
    ' "$payload" >/dev/null
    ;;
  zoho-app | jira-app | linear-app)
    jq -e '
      (.name == env.EXPECTED_NAME or .data != null)
      and ((.data // {}) | keys == ["client_id", "client_secret"])
    ' "$payload" >/dev/null
    ;;
esac

printf '%s|%s\n' "$method" "$url" >>"$CALLS_FILE"
printf '{"name":"%s"}\n' "${RESPONSE_NAME:-$EXPECTED_NAME}" >"$output"
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl"

run_sync() {
  local name="$1"
  local exists="${2:-0}"
  shift 2
  env \
    PATH="$stub_dir:$PATH" \
    ENV=dev \
    ALLOW_VAULT_READS=1 \
    ALLOW_PLATFORM_SECRET_WRITES=1 \
    SECRET_EXISTS="$exists" \
    EXPECTED_NAME="$name" \
    CALLS_FILE="$work_dir/calls" \
    "$@" \
    bash "$script_under_test" "$name" 2>&1
}

test_requires_explicit_write_approval() {
  local name="requires explicit write approval"
  local output status=0
  output="$(
    env PATH="$stub_dir:$PATH" ENV=dev ALLOW_VAULT_READS=1 \
      bash "$script_under_test" github-app 2>&1
  )" || status=$?

  if [ "$status" -ne 1 ] || [[ "$output" != *"write approval required"* ]]; then
    bad "$name" "write ran without approval"
  else
    ok "$name"
  fi
}

test_creates_complete_github_bag_without_leaking() {
  local name="creates complete GitHub bag without leaking"
  : >"$work_dir/calls"
  local output status=0
  output="$(run_sync github-app 0)" || status=$?

  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! rg --fixed-strings --quiet \
    'POST|https://api-dev.agentsfleet.net/v1/workspaces/0190f5a2-4b2d-7c11-8d5e-2a5f31d98210/secrets' \
    "$work_dir/calls"; then
    bad "$name" "create did not use the workspace collection"
  elif [[ "$output" == *provider-secret-sentinel* ]] || [[ "$output" == *agt_t* ]]; then
    bad "$name" "secret material reached output"
  else
    ok "$name"
  fi
}

test_replaces_existing_bag_in_one_put() {
  local name="replaces existing bag in one PUT"
  : >"$work_dir/calls"
  local output status=0
  output="$(run_sync github-app 1)" || status=$?

  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! rg --fixed-strings --quiet \
    'PUT|https://api-dev.agentsfleet.net/v1/workspaces/0190f5a2-4b2d-7c11-8d5e-2a5f31d98210/secrets/github-app' \
    "$work_dir/calls"; then
    bad "$name" "replacement did not use the item route"
  else
    ok "$name"
  fi
}

test_maps_every_provider_field_name() {
  local name="maps every provider field name"
  local secret_name output status
  local -a secret_names=(slack-app zoho-app jira-app linear-app qstash)
  for secret_name in "${secret_names[@]}"; do
    : >"$work_dir/calls"
    status=0
    output="$(run_sync "$secret_name" 0)" || status=$?
    if [ "$status" -ne 0 ]; then
      bad "$name" "$secret_name failed: $output"
      return
    fi
  done
  ok "$name"
}

test_rejects_invalid_inputs_before_writes() {
  local name="rejects invalid inputs before writes"
  local arguments case_entry expected output status
  local -a cases=(
    'github-app|OP_API_KEY=bad|api-key is missing or malformed'
    'github-app|OP_WORKSPACE_ID=bad|is not UUIDv7'
    'slack-app|EMPTY_REF_MATCH=client_secret|empty 1Password field'
    'unknown||unsupported platform secret'
  )
  for case_entry in "${cases[@]}"; do
    arguments="${case_entry#*|}"
    expected="${arguments#*|}"
    arguments="${arguments%%|*}"
    status=0
    if [ -n "$arguments" ]; then
      output="$(run_sync "${case_entry%%|*}" 0 "$arguments")" || status=$?
    else
      output="$(run_sync "${case_entry%%|*}" 0)" || status=$?
    fi
    if [ "$status" -eq 0 ] || [[ "$output" != *"$expected"* ]]; then
      bad "$name" "$case_entry did not fail closed: $output"
      return
    fi
  done
  ok "$name"
}

test_rejects_invalid_environment() {
  local name="rejects invalid environment"
  local output status=0
  output="$(
    env \
      PATH="$stub_dir:$PATH" \
      ENV=staging \
      ALLOW_VAULT_READS=1 \
      ALLOW_PLATFORM_SECRET_WRITES=1 \
      bash "$script_under_test" github-app 2>&1
  )" || status=$?
  if [ "$status" -ne 2 ] || [[ "$output" != *"ENV must be dev or prod"* ]]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}

test_rejects_failed_or_mismatched_api_responses() {
  local name="rejects failed or mismatched API responses"
  local output status=0
  output="$(run_sync github-app 0 CURL_FAIL=1)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a failed list request passed"
    return
  fi
  status=0
  output="$(run_sync github-app 0 RESPONSE_NAME=wrong)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a mismatched write response passed"
  else
    ok "$name"
  fi
}

test_ignores_ambient_api_endpoint_override() {
  local name="ignores ambient API endpoint override"
  local output status=0
  : >"$work_dir/calls"
  output="$(run_sync github-app 0 API_BASE=https://attacker.example)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif rg --quiet attacker.example "$work_dir/calls"; then
    bad "$name" "provider secrets were sent to the ambient endpoint"
  elif ! rg --quiet api-dev.agentsfleet.net "$work_dir/calls"; then
    bad "$name" "the canonical development endpoint was not used"
  else
    ok "$name"
  fi
}

echo "platform-secret sync regression tests"
test_requires_explicit_write_approval
test_creates_complete_github_bag_without_leaking
test_replaces_existing_bag_in_one_put
test_maps_every_provider_field_name
test_rejects_invalid_inputs_before_writes
test_rejects_invalid_environment
test_rejects_failed_or_mismatched_api_responses
test_ignores_ambient_api_endpoint_override

echo ""
echo "results: $passed passed, $failed failed"
test "$failed" -eq 0
