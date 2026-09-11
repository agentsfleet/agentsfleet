#!/usr/bin/env bash
# 01_credential_check.sh - the live strings and the staged `next-*` strings
# both exist, name different hosts, and sit on the right ports.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"
# shellcheck source=./lib.sh
source "$SCRIPT_DIR/lib.sh"

# Reads the vault (op:// refs below), so it carries the same approval + auth
# gates as every other script under playbooks/operations/ (enforced by
# `make check-playbooks`).
playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool op
playbooks_require_tool docker

echo ""
echo "== database-region-move Section 1: credential, host and port check =="
echo ""

env_mode="$(region_move_require_env)"
vault="$(region_move_vault "$env_mode")"
item="$(region_move_item "$env_mode")"
missing=0

# Reads a ref and reports its presence; prints nothing of its value.
read_ref() {
  local ref="$1"
  local value
  value="$(playbooks_read_ref_or_empty "$ref")"
  if [ -z "$value" ]; then
    echo "✗ MISSING: $ref"
    missing=$((missing + 1))
  else
    echo "✓ $ref"
  fi
  printf '%s' "$value"
}

check_port() {
  local value="$1" expected="$2" label="$3"
  local port
  port="$(region_move_url_port "$value")"
  if [ "$port" = "$expected" ]; then
    echo "✓ port $expected: $label"
  else
    echo "✗ INVALID: $label must name port $expected (found: ${port:-none})"
    missing=$((missing + 1))
  fi
}

echo "-- vault: $vault / $item"
live_migrator="$(read_ref "op://$vault/$item/migrator-connection-string")"
next_migrator="$(read_ref "op://$vault/$item/next-migrator-connection-string")"
next_api="$(read_ref "op://$vault/$item/next-api-connection-string")"

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 1 failed: $missing field(s) missing — stage the new branch's strings as next-* first"
  exit 1
fi

live_host="$(region_move_url_host "$live_migrator")"
next_host="$(region_move_url_host "$next_migrator")"
echo "  source host: ${live_host:-unparseable}"
echo "  target host: ${next_host:-unparseable}"

if [ -z "$live_host" ] || [ -z "$next_host" ]; then
  echo "✗ INVALID: a connection string did not parse as a Postgres URL"
  missing=$((missing + 1))
elif [ "$live_host" = "$next_host" ]; then
  echo "✗ INVALID: next-* strings name the live host — they must point at the new branch"
  missing=$((missing + 1))
else
  echo "✓ target is a different host from the source"
fi

if [ "$(region_move_url_host "$next_api")" != "$next_host" ]; then
  echo "✗ INVALID: next-api and next-migrator name different hosts"
  missing=$((missing + 1))
fi

check_port "$live_migrator" "$REGION_MOVE_DIRECT_PORT" "live migrator (direct)"
check_port "$next_migrator" "$REGION_MOVE_DIRECT_PORT" "next migrator (direct)"
check_port "$next_api" "$REGION_MOVE_API_PORT" "next api (PgBouncer)"

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "❌ section 1 failed: $missing issue(s)"
  exit 1
fi

echo ""
echo "✅ section 1 passed"
