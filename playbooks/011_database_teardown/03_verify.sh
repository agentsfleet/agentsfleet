#!/bin/bash
# verify.sh - Verify database teardown completeness
# Single script to check all objects: schemas, tables, indexes, roles, functions, triggers

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../lib/common.sh"

vault_dev="${VAULT_DEV:-ZMB_CD_DEV}"
vault_prod="${VAULT_PROD:-ZMB_CD_PROD}"
env_mode="${ENV:-}"

if [ -z "$env_mode" ]; then
	echo "ERROR: ENV must be set (dev or prod)"
	exit 1
fi

if [ "$env_mode" != "dev" ] && [ "$env_mode" != "prod" ]; then
	echo "ERROR: ENV must be 'dev' or 'prod'"
	exit 1
fi

get_connection_string() {
	local ref="$1"
	playbooks_read_ref_or_empty "$ref"
}

get_password() {
	local url="$1"
	echo "$url" | sed -n 's/.*:\/\/[^:]*:\([^@]*\)@.*/\1/p'
}

verify_database() {
	local url="$1"
	local env_label="$2"
	local password
	local tmp_sql

	password=$(get_password "$url")

	echo ""
	echo "============================================================"
	echo "  TEARDOWN VERIFICATION: $env_label"
	echo "============================================================"

	tmp_sql=$(mktemp)
	cat >"$tmp_sql" <<'SQLEOF'
-- ============================================================
-- TEARDOWN VERIFICATION
-- ============================================================

-- 1. USER SCHEMAS (should be empty)
\echo '--- 1. USER SCHEMAS (should be empty) ---'
SELECT schema_name
FROM information_schema.schemata
WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
AND schema_name NOT LIKE 'pg_%'
AND schema_name NOT LIKE 'pscale%'
ORDER BY schema_name;

-- 2. USER TABLES (should be empty)
\echo '--- 2. USER TABLES (should be empty) ---'
SELECT table_schema || '.' || table_name as table_path
FROM information_schema.tables
WHERE table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
ORDER BY table_schema, table_name;

-- 3. PUBLIC SCHEMA TABLES (should be empty)
\echo '--- 3. PUBLIC SCHEMA TABLES (should be empty) ---'
SELECT table_name
FROM information_schema.tables
WHERE table_schema = 'public'
ORDER BY table_name;

-- 4. APP ROLES (persist - not dropped by DROP SCHEMA)
\echo '--- 4. APP ROLES (these persist, not schema objects) ---'
SELECT rolname
FROM pg_roles
WHERE rolname IN ('api_runtime', 'worker_runtime', 'db_migrator', 
                  'memory_runtime', 'ops_readonly_human', 'ops_readonly_agent')
ORDER BY rolname;

-- 5. FUNCTIONS IN USER SCHEMAS (should be empty)
\echo '--- 5. FUNCTIONS IN USER SCHEMAS (should be empty) ---'
SELECT routine_schema || '.' || routine_name as function_path
FROM information_schema.routines
WHERE routine_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
AND routine_schema NOT LIKE 'pg_%'
ORDER BY routine_schema, routine_name;

-- 6. TRIGGERS IN USER SCHEMAS (should be empty)
\echo '--- 6. TRIGGERS IN USER SCHEMAS (should be empty) ---'
SELECT trigger_schema || '.' || trigger_name as trigger_path
FROM information_schema.triggers
WHERE trigger_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
ORDER BY trigger_schema, trigger_name;

-- 7. INDEXES IN USER SCHEMAS (should be empty)
\echo '--- 7. INDEXES IN USER SCHEMAS (should be empty) ---'
SELECT schemaname || '.' || indexname as index_path
FROM pg_indexes
WHERE schemaname NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
ORDER BY schemaname, indexname;

-- 8. SUMMARY
\echo '--- 8. SUMMARY ---'
SELECT 'User schemas: ' || COUNT(*)::text as status
FROM information_schema.schemata
WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
AND schema_name NOT LIKE 'pg_%'
AND schema_name NOT LIKE 'pscale%'
UNION ALL
SELECT 'User tables: ' || COUNT(*)::text
FROM information_schema.tables
WHERE table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
UNION ALL
SELECT 'App roles: ' || COUNT(*)::text
FROM pg_roles
WHERE rolname IN ('api_runtime', 'worker_runtime', 'db_migrator', 
                  'memory_runtime', 'ops_readonly_human', 'ops_readonly_agent')
UNION ALL
SELECT 'User functions: ' || COUNT(*)::text
FROM information_schema.routines
WHERE routine_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
AND routine_schema NOT LIKE 'pg_%'
UNION ALL
SELECT 'User triggers: ' || COUNT(*)::text
FROM information_schema.triggers
WHERE trigger_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
UNION ALL
SELECT 'User indexes: ' || COUNT(*)::text
FROM pg_indexes
WHERE schemaname NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public');
SQLEOF

	docker run --rm \
		-e PGPASSWORD="$password" \
		-v "$tmp_sql:/verify.sql:ro" \
		postgres:18-alpine \
		psql "$url" \
		-f /verify.sql \
		-t \
		2>&1 | grep -v "^psql:" | grep -v "^SSL connection" | grep -v "^NOTICE:"

	rm -f "$tmp_sql"
}

# Run verification
if [ "$env_mode" = "dev" ]; then
	dev_url=$(get_connection_string "op://$vault_dev/planetscale-dev/migrator-connection-string")
	[ -n "$dev_url" ] && verify_database "$dev_url" "DEVELOPMENT"
fi

if [ "$env_mode" = "prod" ]; then
	prod_url=$(get_connection_string "op://$vault_prod/planetscale-prod/migrator-connection-string")
	[ -n "$prod_url" ] && verify_database "$prod_url" "PRODUCTION"
fi

echo ""
echo "============================================================"
echo "  VERIFICATION COMPLETE"
echo "============================================================"
echo ""
echo "SUCCESS CRITERIA:"
echo "  - User schemas: 0"
echo "  - User tables: 0"
echo "  - User functions: 0"
echo "  - User triggers: 0"
echo "  - User indexes: 0"
echo "  - Public schema tables: 0"
echo "  - App roles: 5-6 (these persist, not dropped by DROP SCHEMA)"
echo ""
echo "NOTE: Roles are database-level objects and persist after"
echo "DROP SCHEMA. To remove them, run: DROP ROLE role_name;"
