#!/usr/bin/env bash
# database-region-move - copy one environment's PlanetScale database into a
# branch in another region and prove the copy, table by table.
#
# Required environment variables:
#   ENV=dev|prod           - one environment; "all" is refused
#   ACTION=check|copy|verify
#                            check  - credentials, hosts and ports only
#                            copy   - check, then copy, then verify
#                            verify - check, then verify an earlier copy
#   ALLOW_VAULT_READS=1    - every step reads 1Password
#   ALLOW_PROVIDER_WRITES=1 - required by ACTION=copy: it writes the target
#
# The console half — creating the branch and staging its strings in the vault
# as `next-*` fields — is 🤠 Indy's, and is written in 001_playbook.md.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "$SCRIPT_DIR/lib.sh"

ACTION="${ACTION:-check}"

env_mode="$(region_move_require_env)" || {
  echo "Usage: ENV=$REGION_MOVE_ENV_DEV|$REGION_MOVE_ENV_PROD ACTION=check|copy|verify ./00_gate.sh" >&2
  exit 1
}
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

case "$ACTION" in
  check)
    run_step "$SCRIPT_DIR/01_credential_check.sh"
    ;;
  copy)
    run_step "$SCRIPT_DIR/01_credential_check.sh"
    run_step "$SCRIPT_DIR/02_copy.sh"
    run_step "$SCRIPT_DIR/03_verify.sh"
    ;;
  verify)
    run_step "$SCRIPT_DIR/01_credential_check.sh"
    run_step "$SCRIPT_DIR/03_verify.sh"
    ;;
  *)
    echo "ERROR: ACTION must be check, copy, or verify" >&2
    exit 2
    ;;
esac

echo "PASS: database-region-move $ACTION completed for $ENV"
