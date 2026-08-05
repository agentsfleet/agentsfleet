#!/usr/bin/env bash
# 01_diff.sh - Read-only catalogue diff against the target environment.
#
# Writes nothing. This is the arm an operator reads before approving rates, and
# it is also the first half of the apply path, so nobody writes rates they have
# not just seen.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "${SCRIPT_DIR}/../../lib/common.sh"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool node
playbooks_require_tool psql

echo ""
echo "== model-catalogue Section 1: catalogue diff (read-only) =="
echo ""

catalogue_export_database_url

root="$(catalogue_repo_root)"
cd "$root"

# No --apply: the script fetches upstream pricing, diffs it against the live
# catalogue and prints. Nothing reaches the database on this path.
node "$CATALOGUE_SEED_SCRIPT"

echo ""
echo "✅ diff complete for env: ${ENV:-} (nothing written)"
