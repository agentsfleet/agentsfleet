#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ENV=dev \
  "$SCRIPT_DIR/../../lib/runner/verify.sh"
