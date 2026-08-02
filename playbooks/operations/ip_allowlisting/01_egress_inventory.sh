#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
env_mode="${ENV:-all}"
max_age_days="${MAX_EGRESS_AGE_DAYS:-7}"
missing=0

check_env() {
  local label="$1"
  local vault="$2"
  local cidr_ref="op://$vault/fly-egress-ips/cidrs"
  local updated_ref="op://$vault/fly-egress-ips/updated-at"
  local cidrs updated_at

  echo "Checking $label Fly.io egress inventory"
  cidrs="$(playbooks_read_ref_or_empty "$cidr_ref")"
  updated_at="$(playbooks_read_ref_or_empty "$updated_ref")"

  if ! playbooks_is_ipv4_cidr_json_array "$cidrs"; then
    echo "INVALID: $cidr_ref must be a non-empty IPv4 CIDR JSON array"
    missing=$((missing + 1))
  else
    echo "OK: $cidr_ref"
  fi

  if ! playbooks_is_recent_utc_timestamp "$updated_at" "$max_age_days"; then
    echo "STALE: $updated_ref must be a recent Coordinated Universal Time timestamp"
    missing=$((missing + 1))
  else
    echo "OK: $updated_ref"
  fi
}

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool jq
playbooks_require_tool python3

case "$env_mode" in
  all)
    check_env development "$vault_dev"
    check_env production "$vault_prod"
    ;;
  dev) check_env development "$vault_dev" ;;
  prod) check_env production "$vault_prod" ;;
  *)
    echo "ERROR: ENV must be all, dev, or prod" >&2
    exit 2
    ;;
esac

[ "$missing" -eq 0 ] || {
  echo "FAIL: egress inventory has $missing issue(s)"
  exit 1
}

echo "PASS: egress inventory"
