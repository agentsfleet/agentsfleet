#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
env_mode="${ENV:-all}"
missing=0

check_ref() {
  local ref="$1"
  if [ -z "$(playbooks_read_ref_or_empty "$ref")" ]; then
    echo "MISSING: $ref"
    missing=$((missing + 1))
  else
    echo "OK: $ref"
  fi
}

check_env() {
  local label="$1"
  local vault="$2"
  local database_item="$3"
  local redis_item="$4"

  echo "Checking $label provider targets"
  check_ref "op://$vault/$database_item/organization"
  check_ref "op://$vault/$database_item/database"
  check_ref "op://$vault/$database_item/service-token"
  check_ref "op://$vault/$redis_item/db-id"
  check_ref "op://$vault/$redis_item/developer-api-email"
  check_ref "op://$vault/$redis_item/developer-api-key"
}

check_distinct() {
  local left_ref="$1"
  local right_ref="$2"
  local label="$3"
  local left right
  left="$(playbooks_read_ref_or_empty "$left_ref")"
  right="$(playbooks_read_ref_or_empty "$right_ref")"
  if [ -n "$left" ] && [ "$left" = "$right" ]; then
    echo "INVALID: development and production $label must differ"
    missing=$((missing + 1))
  fi
}

playbooks_require_vault_read_approval
playbooks_require_op_auth

case "$env_mode" in
  all)
    check_env development "$vault_dev" planetscale-dev upstash-dev
    check_env production "$vault_prod" planetscale-prod upstash-prod
    check_distinct \
      "op://$vault_dev/planetscale-dev/database" \
      "op://$vault_prod/planetscale-prod/database" \
      "PlanetScale databases"
    check_distinct \
      "op://$vault_dev/upstash-dev/db-id" \
      "op://$vault_prod/upstash-prod/db-id" \
      "Upstash database identifiers"
    ;;
  dev) check_env development "$vault_dev" planetscale-dev upstash-dev ;;
  prod) check_env production "$vault_prod" planetscale-prod upstash-prod ;;
  *)
    echo "ERROR: ENV must be all, dev, or prod" >&2
    exit 2
    ;;
esac

[ "$missing" -eq 0 ] || {
  echo "FAIL: provider targets have $missing issue(s)"
  exit 1
}

echo "PASS: provider targets"
