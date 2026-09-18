#!/usr/bin/env bash
# observability - Grafana Dashboard and Alert Playbook
#
# Applies the dashboard and alert rules this directory carries as files, so the
# operator surface is a thing in git rather than clicks in a UI, and so drift
# between the two can be reported instead of argued about.
#
# Required environment variables:
#   ACTION=check|apply|verify    Which arm to run (default: check)
#   ENV=dev|prod                 Target environment (explicit, no "all")
#   ALLOW_VAULT_READS=1          Every arm reads the Grafana credential
#   ALLOW_OBSERVABILITY_WRITES=1 ACTION=apply only
#
# Usage:
#   ACTION=check  ENV=dev ALLOW_VAULT_READS=1 ./00_gate.sh
#   ACTION=apply  ENV=dev ALLOW_VAULT_READS=1 ALLOW_OBSERVABILITY_WRITES=1 ./00_gate.sh
#   ACTION=verify ENV=dev ALLOW_VAULT_READS=1 ./00_gate.sh
#
# Exit: 0 success · 1 step failure · 2 invalid input (before any step runs)
#
# NOTE: No "all" option — development and production resolve to the same Grafana
# stack today, so one environment per invocation is the only way a write says
# which one it meant.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

readonly ACTION_CHECK="check"
readonly ACTION_APPLY="apply"
readonly ACTION_VERIFY="verify"
readonly ENV_DEV="dev"
readonly ENV_PROD="prod"
readonly INVALID_INPUT=2

usage() {
  echo "Usage: ACTION=$ACTION_CHECK|$ACTION_APPLY|$ACTION_VERIFY ENV=$ENV_DEV|$ENV_PROD ALLOW_VAULT_READS=1 ./00_gate.sh" >&2
}

# Validation runs to completion BEFORE any step is dispatched, so an invalid
# invocation cannot reach a vault read, let alone a write.
action="${ACTION:-$ACTION_CHECK}"
case "$action" in
  "$ACTION_CHECK" | "$ACTION_APPLY" | "$ACTION_VERIFY") ;;
  *)
    echo "❌ ERROR: ACTION must be '$ACTION_CHECK', '$ACTION_APPLY', or '$ACTION_VERIFY'" >&2
    usage
    exit "$INVALID_INPUT"
    ;;
esac

env_mode="${ENV:-}"
if [ "$env_mode" != "$ENV_DEV" ] && [ "$env_mode" != "$ENV_PROD" ]; then
  echo "❌ ERROR: ENV must be '$ENV_DEV' or '$ENV_PROD' (never 'all' — one environment per invocation)" >&2
  usage
  exit "$INVALID_INPUT"
fi

if [ "$action" = "$ACTION_APPLY" ] && [ "${ALLOW_OBSERVABILITY_WRITES:-0}" != "1" ]; then
  echo "❌ ERROR: ALLOW_OBSERVABILITY_WRITES=1 required for ACTION=$ACTION_APPLY" >&2
  echo "Run ACTION=$ACTION_VERIFY first and read the drift it reports." >&2
  exit "$INVALID_INPUT"
fi

export OBS_ENV="$env_mode"

run_step() {
  local step="$1"
  if [ ! -x "$step" ]; then
    echo "Not executable: $step" >&2
    exit 1
  fi
  "$step"
}

# Both read-only steps always run, including as the first half of apply: the
# assets are graded and the credentials proven before anything is written.
run_step "$SCRIPT_DIR/01_assets_check.sh"
run_step "$SCRIPT_DIR/02_credentials.sh"

case "$action" in
  "$ACTION_APPLY") run_step "$SCRIPT_DIR/03_apply.sh" ;;
  "$ACTION_VERIFY") run_step "$SCRIPT_DIR/04_verify.sh" ;;
esac

echo "PASS: observability $action completed for $env_mode"
