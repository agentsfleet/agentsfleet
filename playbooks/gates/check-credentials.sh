#!/usr/bin/env bash
# Compatibility shim.
# Canonical entrypoint moved to:
#   playbooks/gates/m2_001/run.sh
# Use `SECTIONS=...` with the canonical entrypoint.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/m2_001/run.sh" "$@"
