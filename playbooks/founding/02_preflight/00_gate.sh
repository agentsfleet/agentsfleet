#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export ENV="${ENV:-all}"
export STAGE="${STAGE:-bootstrap}"
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

case "$ENV" in
  all | dev | prod) ;;
  *)
    echo "Unknown ENV: $ENV (supported: all, dev, prod)" >&2
    exit 2
    ;;
esac

case "$STAGE" in
  bootstrap | deployment) ;;
  *)
    echo "Unknown STAGE: $STAGE (supported: bootstrap, deployment)" >&2
    exit 2
    ;;
esac

"$SCRIPT_DIR/01_tools_and_auth.sh"
"$SCRIPT_DIR/02_credentials.sh"

if [ "$STAGE" = "deployment" ] && [ "$ENV" != "dev" ]; then
  "$SCRIPT_DIR/03_vercel_envs.sh"
fi

echo "✅ 002_preflight check complete (env: $ENV, stage: $STAGE)"
