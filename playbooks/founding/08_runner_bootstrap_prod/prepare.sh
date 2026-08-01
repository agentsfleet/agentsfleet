#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
hosts="${PROD_WORKER_HOSTS:-}"

if [ -z "$hosts" ]; then
  command -v gh >/dev/null 2>&1 || {
    echo "ERROR: set PROD_WORKER_HOSTS or install gh" >&2
    exit 1
  }
  hosts="$(gh variable get PROD_WORKER_HOSTS --repo agentsfleet/agentsfleet)"
fi

printf '%s' "$hosts" | jq -e 'type == "array" and length > 0' >/dev/null
while IFS= read -r item; do
  ENV=prod WORKER_ITEM="$item" \
    "$SCRIPT_DIR/../../lib/runner/prepare.sh"
done < <(printf '%s' "$hosts" | jq -r '.[].vault_key')
