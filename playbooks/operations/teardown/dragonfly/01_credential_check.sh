#!/bin/bash
# 01_credential_check.sh - Verify tools, Fly access and approval before Dragonfly teardown
#
# No vault read happens here or later: the verbs run inside the Dragonfly app
# over `flyctl ssh console` (fly_exec.sh), where the cluster's own password
# lives, so no connection string is fetched to this machine.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=fly_exec.sh
source "${SCRIPT_DIR}/fly_exec.sh"

echo ""
echo "== dragonfly-teardown Section 1: tool, access and approval check =="
echo ""

if [ "${ALLOW_DRAGONFLY_TEARDOWN:-0}" != "1" ]; then
	echo "❌ MISSING APPROVAL: ALLOW_DRAGONFLY_TEARDOWN=1 required for destructive operation" >&2
	echo "   WARNING: This will PERMANENTLY FLUSH all keys from every Dragonfly cluster member!" >&2
	exit 1
fi
echo "✓ ALLOW_DRAGONFLY_TEARDOWN=1 approved"

env_mode="${ENV:-}"
if [ -z "$env_mode" ]; then
	echo "❌ ERROR: ENV must be set explicitly" >&2
	echo "   Usage: ENV=dev ./00_gate.sh   or   ENV=prod ./00_gate.sh" >&2
	exit 1
fi
if [ "$env_mode" != "dev" ] && [ "$env_mode" != "prod" ]; then
	echo "❌ ERROR: ENV must be 'dev' or 'prod' (destructive operations require explicit targeting)" >&2
	exit 1
fi

dragonfly_require_fly_auth
echo "✓ flyctl authenticated"

app="$(dragonfly_app_for_env "$env_mode")"
echo ""
echo "-- the cluster's own view, read inside $app (read-only)"
dragonfly_cluster_run "$app" members

echo ""
echo "✅ section 1 passed - approval granted, $app reachable, every member online"
