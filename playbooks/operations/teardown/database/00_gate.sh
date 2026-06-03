#!/bin/bash
# 011_database_teardown - Database Teardown Playbook
#
# WARNING: DESTRUCTIVE OPERATION
# This playbook permanently deletes all data from PlanetScale databases.
#
# Required environment variables:
#   ALLOW_DATABASE_TEARDOWN=1 - Required to confirm destructive operation
#   ENV=dev|prod             - Target environment (must be explicit, no "all")
#
# Usage:
#   ALLOW_DATABASE_TEARDOWN=1 ENV=dev ./00_gate.sh
#   ALLOW_DATABASE_TEARDOWN=1 ENV=prod ./00_gate.sh
#
# NOTE: No "all" option - must run separately for each environment (safety)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -z "${ENV:-}" ]; then
	echo "❌ ERROR: ENV must be set explicitly (dev or prod)" >&2
	echo "Usage: ALLOW_DATABASE_TEARDOWN=1 ENV=dev ./00_gate.sh" >&2
	exit 1
fi
export ENV
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

for script in "$SCRIPT_DIR"/0[1-9]_*.sh "$SCRIPT_DIR"/[1-9][0-9]_*.sh; do
	[ -f "$script" ] || continue
	[ -x "$script" ] || {
		echo "Not executable: $script" >&2
		exit 1
	}
	"$script"
done

echo "✅ 011_database_teardown complete (env: $ENV)"
