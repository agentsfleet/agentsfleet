#!/bin/bash
# 03_verify.sh - Prove every Dragonfly cluster member is empty (DBSIZE 0 on each)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=fly_exec.sh
source "${SCRIPT_DIR}/fly_exec.sh"

env_mode="${ENV:-}"
if [ -z "$env_mode" ]; then
	echo "ERROR: ENV must be set (dev or prod)"
	exit 1
fi
if [ "$env_mode" != "dev" ] && [ "$env_mode" != "prod" ]; then
	echo "ERROR: ENV must be 'dev' or 'prod'"
	exit 1
fi

app="$(dragonfly_app_for_env "$env_mode")"

echo ""
echo "============================================================"
echo "  TEARDOWN VERIFICATION: $app, every member"
echo "============================================================"

exit_code=0
dragonfly_cluster_run "$app" verify || exit_code=1

echo ""
echo "============================================================"
echo "  VERIFICATION COMPLETE"
echo "============================================================"
echo ""
echo "SUCCESS CRITERIA:"
echo "  - every member the cluster names is online and answers"
echo "  - DBSIZE: 0 on each of them, primaries and replicas"
echo ""
if [ "$exit_code" -eq 0 ]; then
	echo "  ✅ SUCCESS: every member is empty"
else
	echo "  ❌ FAIL: the cluster is not proven empty" >&2
fi
echo ""
echo "NEXT: restart or redeploy every agentsfleetd machine before traffic."
echo "Startup recreates connector:outbound; normal writes recreate"
echo "fleet:{fleet_id}:events, fleet_lease, and the fleet:ready:{p} partitions."

exit "$exit_code"
