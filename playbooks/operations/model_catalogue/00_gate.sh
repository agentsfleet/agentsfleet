#!/usr/bin/env bash
# model-catalogue - Model Catalogue Priming Playbook
#
# core.model_library ships EMPTY by design, and every fleet needs a model. A
# freshly rebuilt environment is therefore deployed but not usable until this
# runs. The tool it wraps (scripts/seed-models.mjs) has always worked; what was
# missing is that it lived in the local-development Makefile fragment, no
# playbook referenced it, and `APPLY=1` wrote billing rates with no approval
# variable and no confirmation of which environment was being written — a lower
# bar than the Redis teardown, which only deletes cache.
#
# Required environment variables:
#   ACTION=diff|apply                Which arm to run
#   ENV=dev|prod                     Target environment (explicit, no "all")
#   ALLOW_VAULT_READS=1              Both arms read the vault
#   ALLOW_MODEL_CATALOGUE_WRITES=1   ACTION=apply only
#
# Usage:
#   ACTION=diff  ENV=dev ALLOW_VAULT_READS=1 ./00_gate.sh
#   ACTION=apply ENV=dev ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 ./00_gate.sh
#
# Exit: 0 success · 1 step failure · 2 invalid input (before any step runs)
#
# NOTE: No "all" option — rate writes run against one environment per
# invocation, the same rule the destructive playbooks follow.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

readonly ACTION_DIFF="diff"
readonly ACTION_APPLY="apply"
readonly ENV_DEV="dev"
readonly ENV_PROD="prod"
readonly INVALID_INPUT=2

usage() {
  echo "Usage: ACTION=diff|apply ENV=dev|prod ALLOW_VAULT_READS=1 ./00_gate.sh" >&2
}

# Input validation runs to completion BEFORE any step is dispatched, so an
# invalid invocation cannot reach a vault read, let alone a write.
action="${ACTION:-}"
if [ "$action" != "$ACTION_DIFF" ] && [ "$action" != "$ACTION_APPLY" ]; then
  echo "❌ ERROR: ACTION must be '$ACTION_DIFF' or '$ACTION_APPLY'" >&2
  usage
  exit "$INVALID_INPUT"
fi

# "all" is rejected by name as well as by the allowlist below, because it is the
# value an operator is most likely to reach for — and catalogue rows are
# billing rates, so writing two environments from one command is never right.
env_mode="${ENV:-}"
if [ "$env_mode" != "$ENV_DEV" ] && [ "$env_mode" != "$ENV_PROD" ]; then
  echo "❌ ERROR: ENV must be '$ENV_DEV' or '$ENV_PROD' (never 'all' — one environment per invocation)" >&2
  usage
  exit "$INVALID_INPUT"
fi

if [ "$action" = "$ACTION_APPLY" ] && [ "${ALLOW_MODEL_CATALOGUE_WRITES:-0}" != "1" ]; then
  echo "❌ ERROR: ALLOW_MODEL_CATALOGUE_WRITES=1 required for ACTION=$ACTION_APPLY" >&2
  echo "Catalogue rows are billing rates. Run ACTION=$ACTION_DIFF first and read the diff." >&2
  exit "$INVALID_INPUT"
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

# The diff arm is read-only and always runs — including as the first half of
# apply, so an operator never writes rates they have not just seen.
run_step "$SCRIPT_DIR/01_diff.sh"

if [ "$action" = "$ACTION_APPLY" ]; then
  run_step "$SCRIPT_DIR/02_apply.sh"
  run_step "$SCRIPT_DIR/03_verify.sh"
fi

echo "✅ model-catalogue $action complete (env: $env_mode)"
