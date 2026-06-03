#!/usr/bin/env bash
# M28_001 §0: Verify Grafana observability credentials exist in vault.
set -euo pipefail

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
