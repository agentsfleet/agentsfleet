#!/bin/bash
# 02_teardown.sh - Execute Redis cache teardown via redis-cli container

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
echo "== redis-teardown Section 2: redis teardown execution =="
echo ""

# Require approval (double-check)
if [ "${ALLOW_REDIS_TEARDOWN:-0}" != "1" ]; then
	echo "ERROR: ALLOW_REDIS_TEARDOWN=1 required" >&2
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

# Confirm + FLUSHALL one cache. The URL is forwarded via env-name-only
# (`-e REDIS_URL`, no value) so the password never appears in `ps aux`.
teardown_redis() {
	local url="$1"
	local env_label="$2"

	echo ""
	echo "================================================"
	echo "TARGET: $env_label"
	echo "================================================"
	echo "⚠️  WARNING: This will PERMANENTLY FLUSH all keys!"
	echo ""

	echo "To proceed with $env_label teardown, type the environment name: $env_label"
	read -r confirmation

	if [ "$confirmation" != "$env_label" ]; then
		echo "❌ Confirmation failed. Expected '$env_label', got '$confirmation'"
		echo "Skipping $env_label teardown."
		return 1
	fi

	echo "Confirmation accepted. Proceeding with teardown..."
	sleep 2
	echo "Connecting to cache..."

	if REDIS_URL="$url" docker run --rm \
		-e REDIS_URL \
		redis:7-alpine \
		sh -c 'redis-cli -u "$REDIS_URL" FLUSHALL'; then
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
	dev_url=$(get_connection_string "op://$vault_dev/upstash-dev/url")
	if [ -n "$dev_url" ]; then
		teardown_redis "$dev_url" "DEVELOPMENT" || exit_code=1
	else
		echo "❌ Failed to read DEV connection string"
		exit_code=1
	fi
fi

if [ "$env_mode" = "prod" ]; then
	prod_url=$(get_connection_string "op://$vault_prod/upstash-prod/url")
	if [ -n "$prod_url" ]; then
		teardown_redis "$prod_url" "PRODUCTION" || exit_code=1
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
