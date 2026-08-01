#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly APPLY="$SCRIPT_DIR/03_planetscale_apply.sh"
readonly VERIFY="$SCRIPT_DIR/04_verify.sh"
readonly INVENTORY="$SCRIPT_DIR/01_egress_inventory.sh"

passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
trap 'rm -rf "$work_dir"' EXIT

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
printf 'op %s\n' "$*" >>"$CALLS"
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read)
    case "${2:-}" in
      */fly-egress-ips/cidrs) printf '["203.0.113.10/32"]\n' ;;
      */fly-egress-ips/updated-at)
        printf '%s\n' "${EGRESS_UPDATED_AT:-2026-07-31T10:00:00Z}"
        ;;
      */organization) printf 'agentsfleet\n' ;;
      */database) printf 'agentsfleet-dev\n' ;;
      */service-token) printf 'planet-secret\n' ;;
      */db-id) printf 'redis-dev-id\n' ;;
      */developer-api-email) printf 'operator@example.test\n' ;;
      */developer-api-key) printf 'upstash-secret\n' ;;
      */allowlist-cidrs) printf '["203.0.113.10/32"]\n' ;;
      */allowlist-verified-at) printf '2026-07-31T10:00:00Z\n' ;;
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
url="${*: -1}"
previous=""
for argument in "$@"; do
  if [ "$previous" = "--request" ]; then method="$argument"; fi
  previous="$argument"
done

case "$url" in
  *api.planetscale.com/v1*)
    case "$method/${PLANETSCALE_LIST_MODE:-empty}" in
      GET/empty) printf '{"data":[]}\n' ;;
      GET/drift)
        printf '{"data":[{"id":"entry-1","schema":"","role":"","cidrs":["198.51.100.1/32"]}]}\n'
        ;;
      GET/same|GET/verify)
        printf '{"data":[{"id":"entry-1","schema":"","role":"","cidrs":["203.0.113.10/32"]}]}\n'
        ;;
      POST/*|PATCH/*)
        printf '{"id":"entry-1","schema":"","role":"","cidrs":["203.0.113.10/32"]}\n'
        ;;
      *) exit 1 ;;
    esac
    ;;
  *api.upstash.com/v2*)
    printf '{"database_id":"redis-dev-id","securityAddons":{"ipWhitelisting":%s}}\n' \
      "${UPSTASH_ENABLED:-true}"
    ;;
  *) exit 1 ;;
esac
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl"

run_script() {
  : >"$calls"
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    ENV=dev \
    ALLOW_VAULT_READS=1 \
    ALLOW_PROVIDER_WRITES=1 \
    PLAYBOOKS_NOW=2026-07-31T12:00:00Z \
    "$@" 2>&1
}

test_should_create_missing_planetscale_entry() {
  local name="test_should_create_missing_planetscale_entry"
  local output status=0
  output="$(run_script PLANETSCALE_LIST_MODE=empty bash "$APPLY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! grep -q -- '--request POST' "$calls"; then
    bad "$name" "create request was not sent"
  elif grep -Eq 'planet-secret|upstash-secret' "$calls"; then
    bad "$name" "a management credential appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_update_drifted_planetscale_entry() {
  local name="test_should_update_drifted_planetscale_entry"
  local output status=0
  output="$(run_script PLANETSCALE_LIST_MODE=drift bash "$APPLY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! grep -q -- '--request PATCH' "$calls"; then
    bad "$name" "update request was not sent"
  else
    ok "$name"
  fi
}

test_should_require_provider_write_approval() {
  local name="test_should_require_provider_write_approval"
  local output status=0
  output="$(run_script ALLOW_PROVIDER_WRITES=0 bash "$APPLY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "PlanetScale writes ran without approval"
  elif [ -s "$calls" ]; then
    bad "$name" "the denied write reached 1Password or a provider"
  else
    ok "$name"
  fi
}

test_should_verify_both_providers() {
  local name="test_should_verify_both_providers"
  local output status=0
  output="$(run_script PLANETSCALE_LIST_MODE=verify bash "$VERIFY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [[ "$output" != *"PASS: development Upstash IP allowlisting"* ]]; then
    bad "$name" "$output"
  elif grep -Eq 'planet-secret|upstash-secret' "$calls"; then
    bad "$name" "a management credential appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_reject_disabled_upstash_allowlisting() {
  local name="test_should_reject_disabled_upstash_allowlisting"
  local output status=0
  output="$(run_script \
    PLANETSCALE_LIST_MODE=verify \
    UPSTASH_ENABLED=false \
    bash "$VERIFY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "disabled Upstash allowlisting passed"
  else
    ok "$name"
  fi
}

test_should_reject_stale_egress_inventory() {
  local name="test_should_reject_stale_egress_inventory"
  local output status=0
  output="$(run_script \
    EGRESS_UPDATED_AT=2026-06-01T00:00:00Z \
    bash "$INVENTORY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "stale inventory passed"
  else
    ok "$name"
  fi
}

test_should_ignore_ambient_provider_endpoint_overrides() {
  local name="test_should_ignore_ambient_provider_endpoint_overrides"
  local output status=0
  output="$(run_script \
    PLANETSCALE_LIST_MODE=empty \
    PLANETSCALE_API_BASE=https://attacker.example \
    bash "$APPLY")" || status=$?
  if [ "$status" -ne 0 ] || rg --quiet attacker.example "$calls" ||
    ! rg --quiet api.planetscale.com "$calls"; then
    bad "$name" "apply used the ambient endpoint: $output"
    return
  fi
  status=0
  output="$(run_script \
    PLANETSCALE_LIST_MODE=verify \
    PLANETSCALE_API_BASE=https://attacker.example \
    UPSTASH_API_BASE=https://attacker.example \
    bash "$VERIFY")" || status=$?
  if [ "$status" -ne 0 ] || rg --quiet attacker.example "$calls" ||
    ! rg --quiet api.planetscale.com "$calls" ||
    ! rg --quiet api.upstash.com "$calls"; then
    bad "$name" "verify used the ambient endpoint: $output"
  else
    ok "$name"
  fi
}

test_should_create_missing_planetscale_entry
test_should_update_drifted_planetscale_entry
test_should_require_provider_write_approval
test_should_verify_both_providers
test_should_reject_disabled_upstash_allowlisting
test_should_reject_stale_egress_inventory
test_should_ignore_ambient_provider_endpoint_overrides

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
