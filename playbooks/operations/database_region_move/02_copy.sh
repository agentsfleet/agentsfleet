#!/usr/bin/env bash
# 02_copy.sh - copy every row from the live branch into the new one.
#
# Data only. The new branch is migrated by `agentsfleetd migrate` BEFORE this
# runs (001_playbook.md, cutover step 2), which creates every schema, table,
# role, grant and trigger the way a deploy would — so the copy never has to
# carry privileges it cannot restore on a managed database, and the ledger it
# would otherwise collide with is excluded and compared instead.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"
# shellcheck source=./lib.sh
source "$SCRIPT_DIR/lib.sh"

# Reads the vault (op:// refs below), so it carries the same approval + auth
# gates as every other script under playbooks/operations/ (enforced by
# `make check-playbooks`). The copy writes a provider database, so it also
# needs the provider-write approval the allowlisting playbook already uses.
playbooks_require_vault_read_approval
playbooks_require_op_auth
playbooks_require_tool docker

readonly POSTGRES_IMAGE="postgres:18-alpine"
readonly LEDGER_TABLES=("audit.schema_migrations" "audit.schema_migration_failures")

echo ""
echo "== database-region-move Section 2: data copy =="
echo ""

if [ "${ALLOW_PROVIDER_WRITES:-0}" != "1" ]; then
  echo "❌ MISSING APPROVAL: ALLOW_PROVIDER_WRITES=1 required — this writes the target branch" >&2
  exit 1
fi
echo "✓ ALLOW_PROVIDER_WRITES=1 approved"

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

# The migration ledger's highest version, read inside the container so the
# URL travels by environment name and never through argv.
ledger_version() {
  local url="$1"
  DATABASE_URL="$url" docker run --rm -e DATABASE_URL "$POSTGRES_IMAGE" \
    sh -c 'psql "$DATABASE_URL" -At -v ON_ERROR_STOP=1 -c "SELECT coalesce(max(version), 0) FROM audit.schema_migrations"'
}

# The target must already carry the source's schema, or the rows have nowhere
# to land. A version behind means step 2 of the cutover was skipped; a version
# ahead means the binary that migrated it is newer than the one serving.
source_version="$(ledger_version "$source_url")"
target_version="$(ledger_version "$target_url")"
echo "  ledger: source=$source_version target=$target_version"
if [ "$source_version" != "$target_version" ] || [ "$target_version" = "0" ]; then
  echo "❌ the target is not migrated to the source's version — run 'agentsfleetd migrate' against next-migrator-connection-string first" >&2
  exit 1
fi
echo "✓ target schema is at the source's ledger version"

echo ""
echo "================================================"
echo "TARGET: $env_mode — $(region_move_url_host "$target_url")"
echo "================================================"
echo "This copies every row of the live database into the new branch."
echo "To proceed, type the environment name: $env_mode"
read -r confirmation
if [ "$confirmation" != "$env_mode" ]; then
  echo "❌ Confirmation failed. Expected '$env_mode', got '$confirmation'"
  exit 1
fi

exclude_args=()
for table in "${LEDGER_TABLES[@]}"; do
  exclude_args+=("--exclude-table=$table")
done

# One container, one volume: the dump never leaves the temp dir, and both
# URLs are forwarded by name so neither password appears in `ps`.
echo "Copying..."
SOURCE_URL="$source_url" TARGET_URL="$target_url" docker run --rm \
  -e SOURCE_URL -e TARGET_URL \
  -v "$work_dir:/work" \
  "$POSTGRES_IMAGE" \
  sh -c 'pg_dump --data-only --format=custom --no-owner --no-privileges "$@" \
           --file=/work/data.dump "$SOURCE_URL" \
         && pg_restore --data-only --no-owner --no-privileges --exit-on-error \
           --dbname="$TARGET_URL" /work/data.dump' \
  sh "${exclude_args[@]}"

echo ""
echo "✅ section 2 passed - rows copied; section 3 proves it"
