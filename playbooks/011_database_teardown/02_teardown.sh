#!/bin/bash
# 02_teardown.sh - Execute database teardown via psql container

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/common.sh
source "${SCRIPT_DIR}/../lib/common.sh"

echo ""
echo "== 011_database_teardown Section 2: database teardown execution =="
echo ""

# Require approval (double-check)
if [ "${ALLOW_DATABASE_TEARDOWN:-0}" != "1" ]; then
	echo "ERROR: ALLOW_DATABASE_TEARDOWN=1 required" >&2
	exit 1
fi

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
env_mode="${ENV:-}"

# Validate ENV - must be explicitly dev or prod, no "all" allowed
if [ -z "$env_mode" ]; then
	echo "ERROR: ENV must be set explicitly" >&2
	echo "Usage: ENV=dev ./00_gate.sh   or   ENV=prod ./00_gate.sh" >&2
	exit 1
fi

if [ "$env_mode" != "dev" ] && [ "$env_mode" != "prod" ]; then
	echo "ERROR: ENV must be 'dev' or 'prod' (destructive operations require explicit targeting)" >&2
	exit 1
fi

# Read connection strings from 1Password
get_connection_string() {
	local ref="$1"
	playbooks_read_ref_or_empty "$ref"
}

# Extract password from PostgreSQL URL
get_password() {
	local url="$1"
	# Parse password from postgresql://user:password@host...
	echo "$url" | sed -n 's/.*:\/\/[^:]*:\([^@]*\)@.*/\1/p'
}

# Strip password from URL so it never shows up in `ps aux`.
# psql will pick up credentials from PGPASSWORD instead.
strip_password() {
	local url="$1"
	echo "$url" | sed -E 's|^(.+://[^:/@]+):[^@]+@|\1@|'
}

# Execute teardown on a database
teardown_database() {
	local url="$1"
	local env_label="$2"
	local password
	local safe_url
	local tmp_sql

	password=$(get_password "$url")
	safe_url=$(strip_password "$url")

	echo ""
	echo "================================================"
	echo "TARGET: $env_label"
	echo "================================================"
	echo "⚠️  WARNING: This will PERMANENTLY DELETE all data!"
	echo ""

	# Interactive confirmation with typed response
	echo "To proceed with $env_label teardown, type the environment name: $env_label"
	read -r confirmation

	if [ "$confirmation" != "$env_label" ]; then
		echo "❌ Confirmation failed. Expected '$env_label', got '$confirmation'"
		echo "Skipping $env_label teardown."
		return 1
	fi

	echo "Confirmation accepted. Proceeding with teardown..."
	sleep 2

	# Use shared teardown SQL file
	tmp_sql="${SCRIPT_DIR}/teardown.sql"

	# Execute using postgres container
	# Using sslmode=require for PlanetScale compatibility
	echo "Connecting to database..."

	if docker run --rm \
		-e PGPASSWORD="$password" \
		-v "$tmp_sql:/teardown.sql:ro" \
		postgres:18-alpine \
		psql "$safe_url" \
		-f /teardown.sql \
		-v ON_ERROR_STOP=1 2>&1; then

		echo ""
		echo "✅ $env_label teardown completed successfully"
	else
		local docker_exit=$?
		echo ""
		echo "❌ $env_label teardown failed with exit code $docker_exit"
		return 1
	fi
}

# Main execution - only one environment at a time (safety: no "all" option)
exit_code=0

if [ "$env_mode" = "dev" ]; then
	dev_url=$(get_connection_string "op://$vault_dev/planetscale-dev/migrator-connection-string")
	if [ -n "$dev_url" ]; then
		teardown_database "$dev_url" "DEVELOPMENT" || exit_code=1
	else
		echo "❌ Failed to read DEV connection string"
		exit_code=1
	fi
fi

if [ "$env_mode" = "prod" ]; then
	prod_url=$(get_connection_string "op://$vault_prod/planetscale-prod/migrator-connection-string")
	if [ -n "$prod_url" ]; then
		teardown_database "$prod_url" "PRODUCTION" || exit_code=1
	else
		echo "❌ Failed to read PROD connection string"
		exit_code=1
	fi
fi

echo ""
echo "================================================"

if [ "$exit_code" -eq 0 ]; then
	echo "✅ section 2 passed - all teardowns completed"
else
	echo "❌ section 2 failed - some teardowns failed"
fi

exit "$exit_code"
