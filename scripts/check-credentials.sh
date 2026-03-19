#!/usr/bin/env bash
# check-credentials.sh — verify all required 1Password vault items exist and are non-empty.
#
# Run from anywhere with op CLI available:
#   ./scripts/check-credentials.sh
#   ENV=prod ./scripts/check-credentials.sh   # check PROD vault only
#   ENV=dev  ./scripts/check-credentials.sh   # check DEV vault only
#
# Vault names: set VAULT_DEV / VAULT_PROD in your environment or as GitHub vars/secrets.
# Requires: op CLI authenticated (op signin or OP_SERVICE_ACCOUNT_TOKEN set)
# See: docs/M2_001_PLAYBOOK_CREDENTIAL_CHECK.md

set -euo pipefail

# Vault names — set VAULT_DEV / VAULT_PROD in your environment or GitHub vars/secrets
VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

ENV="${ENV:-all}"
MISSING=()
CHECKED=0

check() {
  local vault="$1" item="$2" field="$3"
  local ref="op://$vault/$item/$field"
  CHECKED=$((CHECKED + 1))
  if val=$(op read "$ref" 2>/dev/null) && [ -n "$val" ]; then
    printf "  ✓ %s\n" "$ref"
  else
    printf "  ✗ MISSING: %s\n" "$ref"
    MISSING+=("$ref")
  fi
}

echo ""
echo "Checking 1Password vault items..."
echo "  VAULT_DEV=$VAULT_DEV  VAULT_PROD=$VAULT_PROD"
echo ""

if [[ "$ENV" == "all" || "$ENV" == "prod" ]]; then
  echo "=== $VAULT_PROD ==="
  check "$VAULT_PROD" cloudflare-api-token    credential
  check "$VAULT_PROD" npm-publish-token        credential
  check "$VAULT_PROD" vercel-bypass-website    credential
  check "$VAULT_PROD" vercel-bypass-agents     credential
  check "$VAULT_PROD" vercel-bypass-app        credential
  check "$VAULT_PROD" clerk-prod               publishable-key
  check "$VAULT_PROD" clerk-prod               secret-key
  check "$VAULT_PROD" planetscale-prod         connection-string
  check "$VAULT_PROD" upstash-prod             url
  check "$VAULT_PROD" tailscale                authkey
  check "$VAULT_PROD" worker-ssh               private-key
  check "$VAULT_PROD" discord-ci-webhook       credential
  echo ""
fi

if [[ "$ENV" == "all" || "$ENV" == "dev" ]]; then
  echo "=== $VAULT_DEV ==="
  check "$VAULT_DEV" clerk-dev                 publishable-key
  check "$VAULT_DEV" clerk-dev                 secret-key
  check "$VAULT_DEV" vercel-api-token          credential
  check "$VAULT_DEV" planetscale-dev           connection-string
  check "$VAULT_DEV" upstash-dev               url
  echo ""
fi

echo "Checked: $CHECKED items"

if [ ${#MISSING[@]} -eq 0 ]; then
  echo "✅ All vault items present"
  exit 0
else
  echo "❌ ${#MISSING[@]} item(s) missing:"
  for item in "${MISSING[@]}"; do
    echo "   - $item"
  done
  echo ""
  echo "Create missing items in 1Password, then re-run."
  echo "See docs/M2_001_PLAYBOOK_CREDENTIAL_CHECK.md §4.0 for how to generate each value."
  exit 1
fi
