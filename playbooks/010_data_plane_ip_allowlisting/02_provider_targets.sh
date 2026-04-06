#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../lib/common.sh"

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
env_mode="${ENV:-all}"
missing=0

read_ref() {
  local ref="$1"
  playbooks_read_ref_or_empty "$ref"
}

check_ref() {
  local ref="$1"
  local label="$2"
  local value
  value="$(read_ref "$ref")"
  if [ -z "$value" ]; then
    echo "MISSING: $label ($ref)"
    missing=$((missing + 1))
  else
    echo "OK: $label"
  fi
}

check_not_equal() {
  local left_ref="$1"
  local right_ref="$2"
  local label="$3"
  local left right
  left="$(read_ref "$left_ref")"
  right="$(read_ref "$right_ref")"
  if [ -z "$left" ] || [ -z "$right" ]; then
    return
  fi
  if [ "$left" = "$right" ]; then
    echo "INVALID: $label must differ"
    missing=$((missing + 1))
  else
    echo "OK: $label differs"
  fi
}

check_dev() {
  check_ref "op://$vault_dev/planetscale-dev/allowlist-org" "dev planetscale allowlist-org"
  check_ref "op://$vault_dev/planetscale-dev/allowlist-project" "dev planetscale allowlist-project"
  check_ref "op://$vault_dev/upstash-dev/db-id" "dev upstash db-id"
}

check_prod() {
  check_ref "op://$vault_prod/planetscale-prod/allowlist-org" "prod planetscale allowlist-org"
  check_ref "op://$vault_prod/planetscale-prod/allowlist-project" "prod planetscale allowlist-project"
  check_ref "op://$vault_prod/upstash-prod/db-id" "prod upstash db-id"
}

playbooks_require_vault_read_approval
playbooks_require_op_auth

case "$env_mode" in
  all)
    check_dev
    check_prod
    check_not_equal \
      "op://$vault_dev/planetscale-dev/allowlist-org" \
      "op://$vault_prod/planetscale-prod/allowlist-org" \
      "dev/prod planetscale allowlist-org"
    check_not_equal \
      "op://$vault_dev/planetscale-dev/allowlist-project" \
      "op://$vault_prod/planetscale-prod/allowlist-project" \
      "dev/prod planetscale allowlist-project"
    check_not_equal \
      "op://$vault_dev/upstash-dev/db-id" \
      "op://$vault_prod/upstash-prod/db-id" \
      "dev/prod upstash db-id"
    ;;
  dev) check_dev ;;
  prod) check_prod ;;
  *)
    echo "Unknown ENV: $env_mode (supported: all, dev, prod)" >&2
    exit 2
    ;;
esac

if [ "$missing" -gt 0 ]; then
  echo "FAIL: section 2 has $missing issue(s)"
  exit 1
fi

echo "PASS: section 2"
