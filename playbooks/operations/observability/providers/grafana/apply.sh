#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$#" -ne 1 ]; then
  echo "usage: $0 <dev|prod>" >&2
  exit 2
fi
if [ "${ALLOW_OBSERVABILITY_WRITES:-0}" != "1" ]; then
  echo "ERROR: observability write approval required; set ALLOW_OBSERVABILITY_WRITES=1" >&2
  exit 1
fi
export OBS_ENV="$1"

"$SCRIPT_DIR/assets_check.sh"
"$SCRIPT_DIR/credentials.sh"
"$SCRIPT_DIR/prometheus.sh"
"$SCRIPT_DIR/resources.sh"
"$SCRIPT_DIR/alerts.sh"
