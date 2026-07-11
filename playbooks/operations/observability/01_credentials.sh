#!/usr/bin/env bash
# Verify Grafana observability credentials exist in vault.
#
# Reads the vault, so it carries the same approval + auth gates as every other
# script under playbooks/operations/ (enforced by `make check-playbooks`).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth

VAULT="${VAULT_DEV:-ZMB_CD_DEV}"
ITEM="grafana-observability"

echo "Checking vault: $VAULT / $ITEM"

missing=0
for field in grafana-url grafana-sa-token db-readonly-url; do
  val=$(op read "op://$VAULT/$ITEM/$field" 2>/dev/null || echo "")
  if [ -z "$val" ]; then
    echo "  MISSING: $field"
    missing=$((missing + 1))
  else
    echo "  OK: $field (${#val} chars)"
  fi
done

if [ "$missing" -gt 0 ]; then
  echo "FAIL: $missing credential(s) missing"
  exit 1
fi
echo "PASS: all credentials present"
