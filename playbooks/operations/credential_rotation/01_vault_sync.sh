#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
failures=0

case "${ENV:-}" in
  dev)
    runtime_vault="$vault_dev"
    runtime_suffix="dev"
    ;;
  prod)
    runtime_vault="$vault_prod"
    runtime_suffix="prod"
    ;;
  *)
    echo "ERROR: ENV must be dev or prod" >&2
    exit 2
    ;;
esac

check_ref() {
  local ref="$1"
  local value
  value="$(playbooks_read_ref_or_empty "$ref")"
  if [ -z "$value" ]; then
    echo "MISSING: $ref"
    failures=$((failures + 1))
  else
    echo "OK: $ref"
  fi
}

check_ref "op://$runtime_vault/upstash-$runtime_suffix/url"
check_ref "op://$runtime_vault/upstash-$runtime_suffix/api-url"
check_ref "op://$runtime_vault/posthog-$runtime_suffix/credential"

# Deployment-protection values are shared by the Vercel projects and live in
# the production vault even when the target API is development.
check_ref "op://$vault_prod/vercel-bypass-app/credential"
check_ref "op://$vault_prod/vercel-bypass-agents/credential"
check_ref "op://$vault_prod/vercel-bypass-website/credential"

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures credential field(s) missing" >&2
  exit 1
fi

echo "PASS: $ENV vault fields are present"
