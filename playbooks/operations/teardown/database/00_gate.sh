#!/bin/bash
# database-teardown - Database Teardown Playbook
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

run_step() {
	local step="$1"
	if [ ! -x "$step" ]; then
		echo "Not executable: $step" >&2
		exit 1
	fi
	"$step"
}

run_step "$SCRIPT_DIR/01_credential_check.sh"
run_step "$SCRIPT_DIR/02_teardown.sh"
run_step "$SCRIPT_DIR/03_verify.sh"

echo "✅ database-teardown complete (env: $ENV)"
echo ""
echo "NEXT: the database is empty, so core.model_library is empty too and every"
echo "fleet needs a model. Prime the catalogue before calling the environment"
echo "usable:"
echo "  ACTION=diff  ENV=$ENV ALLOW_VAULT_READS=1 \\"
echo "    ./playbooks/operations/model_catalogue/00_gate.sh"
echo "  ACTION=apply ENV=$ENV ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 \\"
echo "    ./playbooks/operations/model_catalogue/00_gate.sh"
