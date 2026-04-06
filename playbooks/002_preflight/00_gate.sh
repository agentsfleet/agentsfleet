#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export ENV="${ENV:-all}"
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

for script in "$SCRIPT_DIR"/0[1-9]_*.sh "$SCRIPT_DIR"/[1-9][0-9]_*.sh; do
  [ -f "$script" ] || continue; [ -x "$script" ] || { echo "Not executable: $script" >&2; exit 1; }
  "$script"
done

echo "✅ 002_preflight check complete (env: $ENV)"
