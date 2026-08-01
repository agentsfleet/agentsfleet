#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$#" -ne 1 ]; then
  echo "usage: $0 <dev|prod>" >&2
  exit 2
fi
export OBS_ENV="$1"

"$SCRIPT_DIR/assets_check.sh"
"$SCRIPT_DIR/credentials.sh"
"$SCRIPT_DIR/prometheus.sh"
