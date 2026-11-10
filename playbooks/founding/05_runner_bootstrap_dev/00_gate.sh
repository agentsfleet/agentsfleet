#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# The shared gate also probes the Fly route and both native TLS carriers.
ENV=dev \
  "$SCRIPT_DIR/../../lib/runner/verify.sh"
