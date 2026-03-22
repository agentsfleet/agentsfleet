#!/usr/bin/env bash
set -euo pipefail

echo ""
echo "== M2_001 Section 1: startup preflight =="
echo "  ENV=${ENV:-all}  VAULT_DEV=${VAULT_DEV:-ZMB_CD_DEV}  VAULT_PROD=${VAULT_PROD:-ZMB_CD_PROD}"

if ! command -v op >/dev/null 2>&1; then
  echo "❌ missing required command: op" >&2
  exit 1
fi
echo "  ✓ op CLI found"

if [ -n "${OP_SERVICE_ACCOUNT_TOKEN:-}" ]; then
  echo "  ✓ OP_SERVICE_ACCOUNT_TOKEN present"
else
  if op whoami >/dev/null 2>&1; then
    echo "  ✓ op session authenticated (interactive)"
  else
    echo "❌ 1Password auth missing: set OP_SERVICE_ACCOUNT_TOKEN or run 'op signin'" >&2
    exit 1
  fi
fi

if command -v gh >/dev/null 2>&1; then
  if gh auth status >/dev/null 2>&1; then
    echo "  ✓ gh auth ok"
  else
    echo "  ! gh installed but not authenticated (non-blocking)"
  fi
fi

