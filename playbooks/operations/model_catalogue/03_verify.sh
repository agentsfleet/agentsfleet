#!/usr/bin/env bash
# 03_verify.sh - Prove the catalogue is non-empty.
#
# The failure this exists for: priming was skipped or silently failed, the
# environment looks deployed, and the first fleet anyone creates has no model to
# run on. An empty catalogue must fail the step loudly and NAME the row count,
# so the deploy cannot be recorded as green on a hope.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "${SCRIPT_DIR}/../../lib/common.sh"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool psql

readonly COUNT_SQL="SELECT count(*) FROM model.platform_models m
WHERE EXISTS (SELECT 1 FROM model.platform_model_prices p
WHERE p.platform_model_id = m.id AND p.prompt_tokens_above = 0
AND p.effective_from <= statement_timestamp()
AND (p.effective_until IS NULL OR statement_timestamp() < p.effective_until));"

echo ""
echo "== model-catalogue Section 3: verify =="
echo ""

catalogue_export_database_url

# -A -t: unaligned, tuples only, so the result is the bare number.
rows="$(PGDATABASE="$DATABASE_URL" psql -X -A -t -v ON_ERROR_STOP=1 -c "$COUNT_SQL" 2>/dev/null | tr -d '[:space:]')"

# A here-string, not a pipe: `grep -q` exits on the match and, under
# `set -o pipefail`, the writer's SIGPIPE would fail the pipeline precisely when
# the count IS well formed.
if ! grep -Eq '^[0-9]+$' <<<"$rows"; then
  echo "❌ ERROR: could not read the currently priced model.platform_models row count (got: '${rows}')" >&2
  exit 1
fi

if [ "$rows" -eq 0 ]; then
  echo "❌ ERROR: model.platform_models has 0 currently priced rows in env: ${ENV:-}" >&2
  echo "Every fleet needs a model, so this environment is not usable yet." >&2
  echo "Run: ACTION=apply ENV=${ENV:-} ALLOW_VAULT_READS=1 \\" >&2
  echo "     ALLOW_MODEL_CATALOGUE_WRITES=1 ./00_gate.sh" >&2
  exit 1
fi

echo "✅ model.platform_models has $rows currently priced row(s) in env: ${ENV:-}"
