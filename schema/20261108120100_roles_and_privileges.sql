-- Datastore roles and the default-privilege baseline. It is ALTER DEFAULT PRIVILEGES because no
-- table exists yet; per-table grants live with each CREATE TABLE.
DO $$
DECLARE
    r text;
BEGIN
    -- The runner holds no datastore credential, so it has no role. api_runtime reaches
    -- memory_runtime only by an explicit SET ROLE (INHERIT FALSE below).
    FOREACH r IN ARRAY ARRAY[
        'db_migrator',
        'api_runtime',
        'memory_runtime',
        'ops_readonly_human',
        'ops_readonly_fleet'
    ]
    LOOP
        -- Roles are cluster-global but the migration lock is per database, so two databases
        -- on one cluster race the CREATE; either error means the role already exists.
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

GRANT ALL ON SCHEMA public, identity, setup, fleet, runner, connector, model,
    billing, vault, memory, audit
TO db_migrator;

-- Runtime roles reach data, never DDL; table grants live with each CREATE TABLE.
GRANT USAGE ON SCHEMA identity, setup, fleet, runner, thread, connector, model, billing,
    vault, audit
TO api_runtime;

GRANT USAGE ON SCHEMA memory TO memory_runtime;

GRANT USAGE ON SCHEMA audit TO ops_readonly_human, ops_readonly_fleet;

-- INHERIT FALSE keeps the membership dormant until `SET ROLE` names it.
GRANT memory_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;

REVOKE CREATE ON SCHEMA public, identity, setup, fleet, runner, connector, model,
    billing, vault, memory, audit
FROM api_runtime, memory_runtime, ops_readonly_human, ops_readonly_fleet;

-- Not FOR ROLE db_migrator: defaults bind to whichever role runs the migration, and FOR ROLE
-- needs a membership a managed non-superuser migrator lacks.
ALTER DEFAULT PRIVILEGES
    IN SCHEMA identity, setup, fleet, runner, connector, model, billing, vault,
        memory, audit
    REVOKE ALL ON TABLES FROM PUBLIC;
