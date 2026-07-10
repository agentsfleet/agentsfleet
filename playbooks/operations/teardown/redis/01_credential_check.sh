#!/bin/bash
# 01_credential_check.sh - Verify credentials and approvals before Redis teardown

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../../lib/common.sh
source "${SCRIPT_DIR}/../../../lib/common.sh"

# Reads the vault (op:// refs below), so it carries the same approval + auth
# gates as every other script under playbooks/operations/ (enforced by
# `make check-playbooks`). Teardown is destructive, so this is on top of the
# ALLOW_*_TEARDOWN approval, not instead of it.
playbooks_require_vault_read_approval
playbooks_require_op_auth

echo ""
echo "== redis-teardown Section 1: credential and approval check =="
echo ""

# Check required tools
playbooks_require_tool op
playbooks_require_tool docker

# Check approval
if [ "${ALLOW_REDIS_TEARDOWN:-0}" != "1" ]; then
	echo "❌ MISSING APPROVAL: ALLOW_REDIS_TEARDOWN=1 required for destructive operation" >&2
	echo "   WARNING: This will PERMANENTLY FLUSH all keys from the Redis cache!" >&2
	echo "   Set this environment variable to proceed with teardown" >&2
	exit 1
fi

echo "✓ ALLOW_REDIS_TEARDOWN=1 approved"

# Verify vault items exist. The root `url` field carries full ACL privileges
# (FLUSHALL is rejected under the restricted `api-url` / `worker-url` roles).
vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
env_mode="${ENV:-}"

# Validate ENV - must be explicitly dev or prod, no "all" allowed
if [ -z "$env_mode" ]; then
	echo "❌ ERROR: ENV must be set explicitly" >&2
	echo "   Usage: ENV=dev ./00_gate.sh   or   ENV=prod ./00_gate.sh" >&2
	exit 1
fi

if [ "$env_mode" != "dev" ] && [ "$env_mode" != "prod" ]; then
	echo "❌ ERROR: ENV must be 'dev' or 'prod' (destructive operations require explicit targeting)" >&2
	exit 1
fi

missing=0

check_ref() {
	local ref="$1"
	local value
	value=$(playbooks_read_ref_or_empty "$ref")
	if [ -z "$value" ]; then
		echo "✗ MISSING: $ref"
		missing=$((missing + 1))
	else
		echo "✓ $ref"
	fi
}

if [ "$env_mode" = "dev" ]; then
	echo ""
	echo "-- checking DEV vault: $vault_dev"
	check_ref "op://$vault_dev/upstash-dev/url"
fi

if [ "$env_mode" = "prod" ]; then
	echo ""
	echo "-- checking PROD vault: $vault_prod"
	check_ref "op://$vault_prod/upstash-prod/url"
fi

if [ "$missing" -gt 0 ]; then
	echo ""
	echo "❌ section 1 failed: $missing credential(s) missing"
	exit 1
fi

echo ""
echo "✅ section 1 passed - all credentials present and approvals granted"
