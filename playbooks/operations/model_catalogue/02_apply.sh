#!/usr/bin/env bash
# 02_apply.sh - The guarded write.
#
# Catalogue rows are billing rates, so this mirrors the Redis teardown's
# approval shape rather than the local Makefile target's: the ALLOW_* variable
# is checked again here (the gate already checked it — a second check means the
# step cannot be run directly without it either), and the operator must type the
# environment name before anything is written.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "${SCRIPT_DIR}/../../lib/common.sh"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool node
playbooks_require_tool psql

echo ""
echo "== model-catalogue Section 2: apply =="
echo ""

# Double-check: the gate validated this, and so does the step, so running the
# step directly cannot bypass the approval.
if [ "${ALLOW_MODEL_CATALOGUE_WRITES:-0}" != "1" ]; then
  echo "ERROR: ALLOW_MODEL_CATALOGUE_WRITES=1 required" >&2
  exit 1
fi

env_label="${ENV:-}"
if [ "$env_label" != "dev" ] && [ "$env_label" != "prod" ]; then
  echo "ERROR: ENV must be 'dev' or 'prod'" >&2
  exit 1
fi

echo "================================================"
echo "TARGET: $env_label"
echo "================================================"
echo "⚠️  This writes BILLING RATES to core.model_library in $env_label."
echo ""
echo "To proceed, type the environment name: $env_label"
read -r confirmation

# Typing the wrong environment is the mistake this catches: an operator who
# means dev and is pointed at prod types "dev" and is refused.
if [ "$confirmation" != "$env_label" ]; then
  echo "❌ Confirmation failed. Expected '$env_label', got '$confirmation'" >&2
  echo "Nothing was written." >&2
  exit 1
fi

catalogue_export_database_url

root="$(catalogue_repo_root)"
cd "$root"

node "$CATALOGUE_SEED_SCRIPT" --apply

echo ""
echo "✅ catalogue applied to env: $env_label"
