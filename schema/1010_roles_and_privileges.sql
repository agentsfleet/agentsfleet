-- Datastore roles and the default-privilege baseline every later slot inherits.
--
-- Runs before any table exists, so the baseline is ALTER DEFAULT PRIVILEGES: a bare
-- `REVOKE … ON ALL TABLES` here would cover the empty set and miss every later table.
-- Per-table grants live with their CREATE TABLE (RULE SGR), never here.

DO $$
DECLARE
    r text;
BEGIN
    -- The runner holds no datastore credential, so no role is named after it.
    -- `memory_runtime` is the one elevation role: api_runtime is a member with
    -- INHERIT FALSE, so reaching `memory` is an explicit `SET ROLE`.
    FOREACH r IN ARRAY ARRAY[
        'db_migrator',
        'api_runtime',
        'memory_runtime',
        'ops_readonly_human',
        'ops_readonly_fleet'
    ]
    LOOP
        -- Roles are cluster-global while the migration lock is per database, so
        -- two databases on one cluster (every test database, staging beside
        -- production) race the CREATE. 42710 is the loser arriving after the
        -- winner committed; 23505 is the loser arriving while the winner was in
        -- flight, failing on pg_authid_rolname_index. Both mean the role exists.
        BEGIN
            EXECUTE format('CREATE ROLE %I NOLOGIN', r);
        EXCEPTION
            WHEN duplicate_object OR unique_violation THEN
                NULL;
        END;
    END LOOP;
END
$$;

REVOKE CREATE ON SCHEMA public FROM PUBLIC;

GRANT ALL ON SCHEMA public, identity, authoring, fleet, runner, connector, model,
    billing, vault, memory, audit
TO db_migrator;

-- Runtime roles reach data, never DDL; table grants live with each CREATE TABLE.
GRANT USAGE ON SCHEMA identity, authoring, fleet, runner, connector, model, billing,
    vault, audit
TO api_runtime;
GRANT USAGE ON SCHEMA memory TO memory_runtime;
GRANT USAGE ON SCHEMA audit TO ops_readonly_human, ops_readonly_fleet;

-- INHERIT FALSE keeps the membership dormant until `SET ROLE` names it.
GRANT memory_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;

REVOKE CREATE ON SCHEMA public, identity, authoring, fleet, runner, connector, model,
    billing, vault, memory, audit
FROM api_runtime, memory_runtime, ops_readonly_human, ops_readonly_fleet;

-- Not `FOR ROLE db_migrator`: default privileges bind to the role that creates the
-- tables, which is whatever role the migrate job connects as, and `FOR ROLE` needs
-- inherited membership a managed non-superuser migrator does not hold (42501).
ALTER DEFAULT PRIVILEGES
    IN SCHEMA identity, authoring, fleet, runner, connector, model, billing, vault,
        memory, audit
    REVOKE ALL ON TABLES FROM PUBLIC;
