#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# common.sh loads ../../../../lib/common.sh before this script reads the vault.
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

obs_select_environment
obs_require_tools
playbooks_require_vault_read_approval
playbooks_require_op_auth

missing=0
for field in \
  grafana-url \
  grafana-sa-token \
  grafana-namespace \
  prometheus-datasource-uid; do
  value="$(playbooks_read_ref_or_empty "op://$OBS_VAULT/grafana-observability/$field")"
  if [ -z "$value" ]; then
    echo "MISSING: $OBS_VAULT / grafana-observability / $field" >&2
    missing=$((missing + 1))
  else
    echo "OK: $OBS_VAULT / grafana-observability / $field"
  fi
done

if [ "$missing" -ne 0 ]; then
  echo "ERROR: $missing Grafana field(s) missing for $OBS_ENVIRONMENT" >&2
  exit 1
fi

echo "PASS: $OBS_ENVIRONMENT Grafana credentials are present"
