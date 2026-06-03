#!/bin/bash
# 01_credential_check.sh - Verify credentials and approvals before teardown

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/common.sh
source "${SCRIPT_DIR}/../lib/common.sh"

echo ""
echo "== 011_database_teardown Section 1: credential and approval check =="
echo ""

# Check required tools
playbooks_require_tool op
playbooks_require_tool docker

# Check approval
if [ "${ALLOW_DATABASE_TEARDOWN:-0}" != "1" ]; then
	echo "❌ MISSING APPROVAL: ALLOW_DATABASE_TEARDOWN=1 required for destructive operation" >&2
	echo "   WARNING: This will PERMANENTLY DELETE all data from the database(s)!" >&2
	echo "   Set this environment variable to proceed with teardown" >&2
	exit 1
fi

echo "✓ ALLOW_DATABASE_TEARDOWN=1 approved"

# Verify vault items exist
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
	check_ref "op://$vault_dev/planetscale-dev/migrator-connection-string"
fi

if [ "$env_mode" = "prod" ]; then
	echo ""
	echo "-- checking PROD vault: $vault_prod"
	check_ref "op://$vault_prod/planetscale-prod/migrator-connection-string"
fi

if [ "$missing" -gt 0 ]; then
	echo ""
	echo "❌ section 1 failed: $missing credential(s) missing"
	exit 1
fi

echo ""
echo "✅ section 1 passed - all credentials present and approvals granted"
