#!/usr/bin/env bash

set -euo pipefail

playbooks_require_tool() {
  local tool="$1"
  command -v "$tool" >/dev/null 2>&1 || {
    echo "ERROR: required tool not found: $tool" >&2
    exit 1
  }
}

playbooks_require_vault_read_approval() {
  if [ "${ALLOW_VAULT_READS:-0}" != "1" ]; then
    echo "ERROR: vault read approval required. Set ALLOW_VAULT_READS=1." >&2
    exit 1
  fi
}

playbooks_require_op_auth() {
  playbooks_require_tool op
  op whoami >/dev/null 2>&1 || {
    echo "ERROR: op not authenticated; run 'op signin'" >&2
    exit 1
  }
}

playbooks_read_ref_or_empty() {
  local ref="$1"
  op read "$ref" 2>/dev/null || true
}

playbooks_is_ipv4_cidr_json_array() {
  local payload="$1"
  printf '%s' "$payload" | jq -e '
    type == "array" and
    length > 0 and
    all(.[]; type == "string" and test("^(25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})(\\.(25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})){3}/([0-9]|[12][0-9]|3[0-2])$"))
  ' >/dev/null
}
