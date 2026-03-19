#!/usr/bin/env bash
# redis-bootstrap.sh — idempotent Redis stream + ACL setup per environment.
#
# Run once per environment after Redis is provisioned.
# Safe to re-run — XGROUP CREATE is guarded, ACL SETUSER is idempotent.
#
# Usage:
#   ENV=dev  ./schema/redis-bootstrap.sh
#   ENV=prod ./schema/redis-bootstrap.sh
#
# Vault names: set VAULT_DEV / VAULT_PROD in your environment or as GitHub vars/secrets.
# Requires: redis-cli, op CLI authenticated

set -euo pipefail

# Vault names — set VAULT_DEV / VAULT_PROD in your environment or GitHub vars/secrets
VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

ENV="${ENV:-dev}"

case "$ENV" in
  dev)
    VAULT="$VAULT_DEV"
    REDIS_ITEM="upstash-dev"
    ;;
  prod)
    VAULT="$VAULT_PROD"
    REDIS_ITEM="upstash-prod"
    ;;
  *)
    echo "Usage: ENV=dev|prod $0" >&2
    exit 1
    ;;
esac

echo "Loading credentials from 1Password vault: $VAULT"
REDIS_URL=$(op read "op://$VAULT/$REDIS_ITEM/url")
API_PASS=$(op read "op://$VAULT/redis-acl-api-user/credential")
WORKER_PASS=$(op read "op://$VAULT/redis-acl-worker-user/credential")

echo "Connecting to Redis ($ENV)..."
redis-cli -u "$REDIS_URL" PING | grep -q PONG || { echo "✗ Redis connection failed"; exit 1; }
echo "✓ Redis connected"

echo ""
echo "Bootstrapping stream..."
# XGROUP CREATE is idempotent with MKSTREAM — fails silently if group already exists
redis-cli -u "$REDIS_URL" XGROUP CREATE run_queue workers 0 MKSTREAM 2>/dev/null || true
echo "✓ run_queue stream + workers consumer group"

echo ""
echo "Setting ACL users..."
redis-cli -u "$REDIS_URL" ACL SETUSER api_user \
  on ">$API_PASS" \
  "~run_queue" \
  "+xadd" "+xgroup" "+xlen" "+xrange" "+ping"
echo "✓ api_user"

redis-cli -u "$REDIS_URL" ACL SETUSER worker_user \
  on ">$WORKER_PASS" \
  "~run_queue" \
  "+xreadgroup" "+xack" "+xautoclaim" "+xgroup" "+xlen" "+xinfo" "+ping"
echo "✓ worker_user"

redis-cli -u "$REDIS_URL" ACL SETUSER default off
echo "✓ default user disabled"

echo ""
echo "Verifying ACL list..."
redis-cli -u "$REDIS_URL" ACL LIST | grep -E "api_user|worker_user"

echo ""
echo "✅ Redis bootstrap complete ($ENV)"
