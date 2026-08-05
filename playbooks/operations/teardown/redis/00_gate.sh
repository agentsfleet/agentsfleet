#!/bin/bash
# redis-teardown - Redis Cache Teardown Playbook
#
# WARNING: DESTRUCTIVE OPERATION
# This playbook permanently flushes all keys from the Upstash Redis cache.
#
# Required environment variables:
#   ALLOW_REDIS_TEARDOWN=1 - Required to confirm destructive operation
#   ENV=dev|prod           - Target environment (must be explicit, no "all")
#
# Usage:
#   ALLOW_REDIS_TEARDOWN=1 ENV=dev ./00_gate.sh
#   ALLOW_REDIS_TEARDOWN=1 ENV=prod ./00_gate.sh
#
# NOTE: No "all" option - must run separately for each environment (safety)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -z "${ENV:-}" ]; then
	echo "❌ ERROR: ENV must be set explicitly (dev or prod)" >&2
	echo "Usage: ALLOW_REDIS_TEARDOWN=1 ENV=dev ./00_gate.sh" >&2
	exit 1
fi
export ENV
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

run_step() {
	local step="$1"
	if [ ! -x "$step" ]; then
		echo "Not executable: $step" >&2
		exit 1
	fi
	"$step"
}

# Stopping the writers comes FIRST, before credentials are even checked: a
# live agentsfleetd restarted by Fly.io against a just-flushed cache repopulates
# it, so a teardown that runs while a writer is up is not a teardown. Its
# non-zero exit halts this list, which is the point — it is a gate step, not a
# documented precondition anyone has to remember.
run_step "$SCRIPT_DIR/stop_writers.sh"
run_step "$SCRIPT_DIR/01_credential_check.sh"
run_step "$SCRIPT_DIR/02_teardown.sh"
run_step "$SCRIPT_DIR/03_verify.sh"

echo "✅ redis-teardown complete (env: $ENV)"
