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

check_text_ref() {
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

check_cidr_array_ref() {
  local ref="$1"
  local label="$2"
  local value
  value="$(read_ref "$ref")"
  if [ -z "$value" ]; then
    echo "MISSING: $label ($ref)"
    missing=$((missing + 1))
    return
  fi
  if ! playbooks_is_ipv4_cidr_json_array "$value"; then
    echo "INVALID: $label must be a non-empty IPv4 CIDR JSON array"
    missing=$((missing + 1))
    return
  fi
  echo "OK: $label"
}

check_env() {
  local label="$1"
  local vault="$2"
  echo "== checking $label ($vault) =="
  check_cidr_array_ref "op://$vault/fly-egress-ips/cidrs" "$label fly-egress-ips/cidrs"
  check_text_ref "op://$vault/fly-egress-ips/updated-at" "$label fly-egress-ips/updated-at"
  check_cidr_array_ref "op://$vault/ovh-worker-egress-ips/cidrs" "$label ovh-worker-egress-ips/cidrs"
  check_text_ref "op://$vault/ovh-worker-egress-ips/updated-at" "$label ovh-worker-egress-ips/updated-at"
}

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool jq

case "$env_mode" in
  all)
    check_env "dev" "$vault_dev"
    check_env "prod" "$vault_prod"
    ;;
  dev) check_env "dev" "$vault_dev" ;;
  prod) check_env "prod" "$vault_prod" ;;
  *)
    echo "Unknown ENV: $env_mode (supported: all, dev, prod)" >&2
    exit 2
    ;;
esac

if [ "$missing" -gt 0 ]; then
  echo "FAIL: section 1 has $missing issue(s)"
  exit 1
fi

echo "PASS: section 1"
