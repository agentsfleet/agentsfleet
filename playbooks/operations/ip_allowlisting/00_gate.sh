#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACTION="${ACTION:-check}"

"$SCRIPT_DIR/01_egress_inventory.sh"
"$SCRIPT_DIR/02_provider_targets.sh"

case "$ACTION" in
  check) ;;
  apply)
    "$SCRIPT_DIR/03_planetscale_apply.sh"
    "$SCRIPT_DIR/04_verify.sh"
    ;;
  verify)
    "$SCRIPT_DIR/04_verify.sh"
    ;;
  *)
    echo "ERROR: ACTION must be check, apply, or verify" >&2
    exit 2
    ;;
esac

echo "PASS: IP allowlisting $ACTION completed for ${ENV:-all}"
