#!/bin/bash
# 03_verify.sh - Verify Redis cache teardown completeness (DBSIZE should be 0)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../../../lib/common.sh"

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
env_mode="${ENV:-}"

if [ -z "$env_mode" ]; then
	echo "ERROR: ENV must be set (dev or prod)"
	exit 1
fi

if [ "$env_mode" != "dev" ] && [ "$env_mode" != "prod" ]; then
	echo "ERROR: ENV must be 'dev' or 'prod'"
	exit 1
fi

get_connection_string() {
	local ref="$1"
	playbooks_read_ref_or_empty "$ref"
}

verify_redis() {
	local url="$1"
	local env_label="$2"
	local dbsize

	echo ""
	echo "============================================================"
	echo "  TEARDOWN VERIFICATION: $env_label"
	echo "============================================================"

	# Forward URL via env-name-only so the password never appears in `ps aux`.
	dbsize=$(REDIS_URL="$url" docker run --rm \
		-e REDIS_URL \
		redis:7-alpine \
		sh -c 'redis-cli -u "$REDIS_URL" DBSIZE' | tr -d '[:space:]')

	echo "  Keys remaining (DBSIZE): ${dbsize:-unknown}"

	if [ "$dbsize" = "0" ]; then
		echo "  ✅ SUCCESS: cache is empty"
	else
		echo "  ❌ FAIL: $dbsize key(s) still present"
		return 1
	fi
}

exit_code=0

if [ "$env_mode" = "dev" ]; then
	dev_url=$(get_connection_string "op://$vault_dev/upstash-dev/url")
	[ -n "$dev_url" ] && { verify_redis "$dev_url" "DEVELOPMENT" || exit_code=1; }
fi

if [ "$env_mode" = "prod" ]; then
	prod_url=$(get_connection_string "op://$vault_prod/upstash-prod/url")
	[ -n "$prod_url" ] && { verify_redis "$prod_url" "PRODUCTION" || exit_code=1; }
fi

echo ""
echo "============================================================"
echo "  VERIFICATION COMPLETE"
echo "============================================================"
echo ""
echo "SUCCESS CRITERIA:"
echo "  - DBSIZE: 0 (all keys flushed)"
echo ""
echo "NOTE: FLUSHALL removes the per-zombie event streams and their"
echo "zombie_lease consumer groups. No manual re-priming is needed —"
echo "agentsfleetd recreates each one on demand at zombie-create time."

exit "$exit_code"
