#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERIFY="$SCRIPT_DIR/verify.sh"
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
  read)
    case "${2:-}" in
      */tailscale-hostname) printf '%s\n' "${STUB_RUNNER_HOST:-runner-host}" ;;
      */deploy-user) printf '%s\n' "${STUB_RUNNER_USER:-runner-user}" ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/tailscale" <<'STUB'
#!/usr/bin/env bash
printf 'tailscale %s\n' "$*" >>"$CALLS"
exit 1
STUB

chmod +x "$stub_dir/op" "$stub_dir/tailscale"

run_verify() {
  : >"$calls"
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    ENV=dev \
    ALLOW_VAULT_READS=1 \
    "$@" \
    bash "$VERIFY" 2>&1
}

assert_rejected_before_connection() {
  local name="$1"
  local expected="$2"
  shift 2
  local output status=0
  output="$(run_verify "$@")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "unsafe runner input passed"
  elif [[ "$output" != *"$expected"* ]]; then
    bad "$name" "failure omitted '$expected': $output"
  elif rg --quiet '^tailscale ' "$calls"; then
    bad "$name" "unsafe runner input reached Tailscale"
  else
    ok "$name"
  fi
}

test_should_reject_unsafe_vault_and_item_names() {
  assert_rejected_before_connection \
    "test_should_reject_unsafe_vault_name" \
    "invalid runner vault" \
    VAULT_DEV='bad/vault'
  assert_rejected_before_connection \
    "test_should_reject_unsafe_item_name" \
    "invalid runner item" \
    WORKER_ITEM='bad/item'
}

test_should_reject_unsafe_target_components() {
  assert_rejected_before_connection \
    "test_should_reject_unsafe_deploy_user" \
    "invalid runner deploy user" \
    STUB_RUNNER_USER="runner'; false #"
  assert_rejected_before_connection \
    "test_should_reject_unsafe_tailscale_hostname" \
    "invalid runner Tailscale hostname" \
    STUB_RUNNER_HOST='-oProxyCommand=false'
}

test_should_reject_noncanonical_api_endpoint() {
  assert_rejected_before_connection \
    "test_should_reject_noncanonical_api_endpoint" \
    "runner API URL must match the dev endpoint" \
    AGENTSFLEET_API_URL=https://attacker.example
}

test_should_reject_unsafe_vault_and_item_names
test_should_reject_unsafe_target_components
test_should_reject_noncanonical_api_endpoint

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
