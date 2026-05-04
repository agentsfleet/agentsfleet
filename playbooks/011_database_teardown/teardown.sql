-- COMPLETE DATABASE TEARDOWN
-- Drops all user schemas and their contents
-- For PlanetScale (via psql) or local Docker containers

-- Drop all user-created schemas with CASCADE
-- This removes tables, views, indexes, functions, triggers, etc.
DROP SCHEMA IF EXISTS memory CASCADE;
DROP SCHEMA IF EXISTS vault CASCADE;
DROP SCHEMA IF EXISTS ops_ro CASCADE;
DROP SCHEMA IF EXISTS audit CASCADE;
DROP SCHEMA IF EXISTS billing CASCADE;
DROP SCHEMA IF EXISTS agent CASCADE;
DROP SCHEMA IF EXISTS core CASCADE;

-- Drop any remaining tables in public schema (excluding system tables)
DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT tablename
        FROM pg_tables
        WHERE schemaname = 'public'
        AND tablename NOT LIKE 'pg_%'
        AND tablename NOT LIKE 'sql_%'
    LOOP
        EXECUTE 'DROP TABLE IF EXISTS public.' || quote_ident(r.tablename) || ' CASCADE';
    END LOOP;
END $$;

-- Verification - count remaining user objects.
-- Filters must match between schema_count and table_count, otherwise
-- PlanetScale-managed schemas (pscale_extensions and friends) get
-- excluded from one count but not the other and the check trips on a
-- successful teardown.
DO $$
DECLARE
    schema_count INT;
    table_count INT;
BEGIN
    SELECT COUNT(*) INTO schema_count
    FROM information_schema.schemata
    WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
    AND schema_name NOT LIKE 'pg_%'
    AND schema_name NOT LIKE 'pscale%';

    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables
    WHERE table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast', 'public')
    AND table_schema NOT LIKE 'pg_%'
    AND table_schema NOT LIKE 'pscale%';

    RAISE NOTICE 'Teardown complete: % user schemas remain, % user tables remain', schema_count, table_count;

    IF schema_count = 0 AND table_count = 0 THEN
        RAISE NOTICE 'SUCCESS: All user schemas and tables have been removed';
    ELSE
        RAISE EXCEPTION 'Teardown incomplete: % schemas and % tables remain', schema_count, table_count;
    END IF;
END $$;
