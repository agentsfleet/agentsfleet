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
case "$ref" in
  */agentsfleet-admin/api-key)
    printf 'agt_t%s\n' "$(printf 'a%.0s' {1..64})"
    ;;
  */agentsfleet-admin/platform_admin_workspace_id)
    printf '0190f5a2-4b2d-7c11-8d5e-2a5f31d98210\n'
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
esac

printf '%s|%s\n' "$method" "$url" >>"$CALLS_FILE"
printf '{"name":"%s"}\n' "$EXPECTED_NAME" >"$output"
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl"

run_sync() {
  local name="$1"
  local exists="${2:-0}"
  env \
    PATH="$stub_dir:$PATH" \
    ENV=dev \
    ALLOW_VAULT_READS=1 \
    ALLOW_PLATFORM_SECRET_WRITES=1 \
    SECRET_EXISTS="$exists" \
    EXPECTED_NAME="$name" \
    CALLS_FILE="$work_dir/calls" \
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

test_maps_qstash_field_names() {
  local name="maps QStash field names"
  : >"$work_dir/calls"
  local output status=0
  output="$(run_sync qstash 0)" || status=$?
  if [ "$status" -eq 0 ]; then
    ok "$name"
  else
    bad "$name" "$output"
  fi
}

echo "platform-secret sync regression tests"
test_requires_explicit_write_approval
test_creates_complete_github_bag_without_leaking
test_replaces_existing_bag_in_one_put
test_maps_qstash_field_names

echo ""
echo "results: $passed passed, $failed failed"
test "$failed" -eq 0
