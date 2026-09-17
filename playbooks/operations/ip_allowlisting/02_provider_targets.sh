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

# PlanetScale only. The datastore used to be here too: a hosted service with
# a public endpoint needs its egress ranges allowlisted, and the credentials
# above were how this script reached its management API to read them.
#
# Self-hosted Dragonfly has no public endpoint. It runs on one Fly machine
# reached over 6PN, which is a private network with no ingress from anywhere
# else, so there is no range to allowlist and no management API to ask. The
# control is not weakened by removing it; it is answered by the deployment.
check_env() {
  local label="$1"
  local vault="$2"
  local database_item="$3"

  echo "Checking $label provider targets"
  check_ref "op://$vault/$database_item/organization"
  check_ref "op://$vault/$database_item/database"
  check_ref "op://$vault/$database_item/service-token"
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
    check_env development "$vault_dev" planetscale-dev
    check_env production "$vault_prod" planetscale-prod
    check_distinct \
      "op://$vault_dev/planetscale-dev/database" \
      "op://$vault_prod/planetscale-prod/database" \
      "PlanetScale databases"
    ;;
  dev) check_env development "$vault_dev" planetscale-dev ;;
  prod) check_env production "$vault_prod" planetscale-prod ;;
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
