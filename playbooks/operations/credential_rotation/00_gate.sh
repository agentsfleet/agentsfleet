#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

case "${ENV:-}" in
  dev | prod) ;;
  *)
    echo "ERROR: ENV must be dev or prod" >&2
    exit 2
    ;;
esac

export ENV
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

run_step() {
  local step="$1"
  if [ ! -x "$step" ]; then
    echo "ERROR: not executable: $step" >&2
    exit 1
  fi
  "$step"
}

run_step "$SCRIPT_DIR/01_vault_sync.sh"
run_step "$SCRIPT_DIR/02_service_health.sh"

echo "PASS: $ENV credential rotation verification"
