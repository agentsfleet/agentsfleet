#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool curl
playbooks_require_tool jq

vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
failures=0
bypass_config=""

cleanup() {
  if [ -n "$bypass_config" ] && [ -f "$bypass_config" ]; then
    rm -f "$bypass_config"
  fi
}
trap cleanup EXIT

case "${ENV:-}" in
  dev)
    api_base="https://api-dev.agentsfleet.net"
    app_base="https://app-dev.agentsfleet.net"
    ;;
  prod)
    api_base="https://api.agentsfleet.net"
    app_base="https://app.agentsfleet.net"
    ;;
  *)
    echo "ERROR: ENV must be dev or prod" >&2
    exit 2
    ;;
esac

if curl -fsS --max-time 10 "$api_base/healthz" >/dev/null; then
  echo "OK: $api_base/healthz"
else
  echo "FAIL: $api_base/healthz" >&2
  failures=$((failures + 1))
fi

if curl -fsS --max-time 10 "$api_base/readyz" |
  jq -e '.ready == true' >/dev/null; then
  echo "OK: $api_base/readyz"
else
  echo "FAIL: $api_base/readyz" >&2
  failures=$((failures + 1))
fi

if [ "$ENV" = "dev" ]; then
  bypass_secret="$(
    playbooks_read_ref_or_empty \
      "op://$vault_prod/vercel-bypass-app/credential"
  )"
  if [ -z "$bypass_secret" ] ||
    [[ "$bypass_secret" == *$'\n'* ]] ||
    [[ "$bypass_secret" == *'"'* ]]; then
    echo "FAIL: Vercel bypass credential is missing or malformed" >&2
    failures=$((failures + 1))
  else
    bypass_config="$(mktemp)"
    chmod 600 "$bypass_config"
    printf 'header = "x-vercel-protection-bypass: %s"\n' \
      "$bypass_secret" >"$bypass_config"
    unset bypass_secret
    if curl --config "$bypass_config" \
      --fail \
      --silent \
      --show-error \
      --max-time 10 \
      --output /dev/null \
      "$app_base/sign-in"; then
      echo "OK: $app_base/sign-in with deployment protection"
    else
      echo "FAIL: $app_base/sign-in with deployment protection" >&2
      failures=$((failures + 1))
    fi
  fi
elif curl -fsS --max-time 10 --output /dev/null "$app_base/sign-in"; then
  echo "OK: $app_base/sign-in"
else
  echo "FAIL: $app_base/sign-in" >&2
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures service check(s) failed" >&2
  exit 1
fi

echo "PASS: $ENV services use the rotated credentials"
