#!/usr/bin/env bash
# 03_verify.sh - every user table holds the same number of rows on both
# branches, and both ledgers stand at the same version.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"
# shellcheck source=./lib.sh
source "$SCRIPT_DIR/lib.sh"

# Reads the vault (op:// refs below), so it carries the same approval + auth
# gates as every other script under playbooks/operations/ (enforced by
# `make check-playbooks`).
playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool docker

readonly POSTGRES_IMAGE="postgres:18-alpine"

echo ""
echo "== database-region-move Section 3: verification =="
echo ""

env_mode="$(region_move_require_env)"
vault="$(region_move_vault "$env_mode")"
item="$(region_move_item "$env_mode")"

source_url="$(region_move_with_system_roots "$(playbooks_read_ref_or_empty "op://$vault/$item/migrator-connection-string")")"
target_url="$(region_move_with_system_roots "$(playbooks_read_ref_or_empty "op://$vault/$item/next-migrator-connection-string")")"
[ -n "$source_url" ] && [ -n "$target_url" ] || {
  echo "❌ connection strings missing — run ACTION=check first" >&2
  exit 1
}

work_dir="$(mktemp -d)"
chmod 700 "$work_dir"
trap 'rm -rf -- "$work_dir"' EXIT

# Catalog-derived, never a hand-kept table list: a static list rots the moment
# a migration adds a table, and would then verify a subset. `\gexec` runs the
# generated `count(*)` per table; the ledger rides along as one more line.
cat >"$work_dir/census.sql" <<'SQL'
SELECT format('SELECT %L || ''='' || count(*) FROM %I.%I',
              table_schema || '.' || table_name, table_schema, table_name)
FROM information_schema.tables
WHERE table_type = 'BASE TABLE'
  AND table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
  AND table_schema NOT LIKE 'pg\_%'
  AND table_schema NOT LIKE 'pscale%'
ORDER BY 1 \gexec
SELECT 'ledger.version=' || coalesce(max(version), 0) FROM audit.schema_migrations;
SQL

census() {
  local url="$1" out="$2"
  DATABASE_URL="$url" docker run --rm -e DATABASE_URL \
    -v "$work_dir/census.sql:/census.sql:ro" \
    "$POSTGRES_IMAGE" \
    sh -c 'psql "$DATABASE_URL" -At -v ON_ERROR_STOP=1 -f /census.sql' |
    sort >"$out"
}

census "$source_url" "$work_dir/source.census"
census "$target_url" "$work_dir/target.census"

tables="$(grep -c -v '^ledger\.' "$work_dir/source.census" || true)"
echo "  source: $tables table(s), $(grep '^ledger\.' "$work_dir/source.census")"
echo "  target: $(grep -c -v '^ledger\.' "$work_dir/target.census" || true) table(s), $(grep '^ledger\.' "$work_dir/target.census")"

if [ "$tables" = "0" ]; then
  echo "❌ the source census is empty — the census query matched no user tables" >&2
  exit 1
fi

if ! diff -u "$work_dir/source.census" "$work_dir/target.census"; then
  echo ""
  echo "❌ section 3 failed: the branches differ (above: - source, + target)" >&2
  echo "   A missing table means the target was migrated by a different binary;" >&2
  echo "   a row-count gap means the copy was partial or the source kept writing." >&2
  exit 1
fi

echo ""
echo "✅ section 3 passed - every table and the ledger match"
