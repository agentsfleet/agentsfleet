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
# ENV accepts exactly "dev" or "prod". This gate validates the value before
# dispatching any step, so passing it means ENV is one of those two.
#
# NOTE: No "all" option - must run separately for each environment (safety)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

readonly ENV_DEV="dev"
readonly ENV_PROD="prod"

usage() {
	echo "Usage: ALLOW_DATABASE_TEARDOWN=1 ENV=$ENV_DEV|$ENV_PROD ./00_gate.sh" >&2
	echo "ENV accepts exactly '$ENV_DEV' or '$ENV_PROD' - no \"all\"" >&2
}

# This gate validates the VALUE of ENV, not merely its presence, because it
# presents itself as the gate: a reader of this file - or a future step
# dispatched from it - would otherwise be entitled to believe that passing
# 00_gate.sh means ENV was checked. It did not; only the later steps checked.
#
# 01_credential_check.sh and 02_teardown.sh keep their own identical checks.
# Each is separately runnable and destructive, so each carries its own guard;
# this is a layer added at the entry point, not a check relocated to it.
env_mode="${ENV:-}"
if [ -z "$env_mode" ]; then
	echo "❌ ERROR: ENV must be set explicitly ($ENV_DEV or $ENV_PROD)" >&2
	usage
	exit 1
fi

if [ "$env_mode" != "$ENV_DEV" ] && [ "$env_mode" != "$ENV_PROD" ]; then
	echo "❌ ERROR: ENV must be '$ENV_DEV' or '$ENV_PROD' (destructive operations require explicit targeting)" >&2
	usage
	exit 1
fi

export ENV="$env_mode"
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
