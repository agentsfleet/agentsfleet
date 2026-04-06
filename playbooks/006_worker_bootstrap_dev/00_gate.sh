#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"

for script in "$SCRIPT_DIR"/[0-9][0-9]_*.sh; do
  [ -x "$script" ] || { echo "Not executable: $script" >&2; exit 1; }
  "$script"
done

echo "✅ 006_worker_bootstrap_dev gate complete"
