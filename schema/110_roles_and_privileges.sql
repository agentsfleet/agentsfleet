-- Substrate, part two: datastore roles and the default-privilege baseline every
-- later slot inherits.
--
-- Running before any table exists is what makes the posture below correct rather
-- than decorative. A bare `REVOKE … ON ALL TABLES` here would apply to the empty
-- set and silently miss every table created afterwards, so the baseline is
-- expressed as ALTER DEFAULT PRIVILEGES, which attaches to tables that do not
-- exist yet. The pre-rebuild schema carried the bare form and therefore only
-- ever covered the two tables that existed at that point.
--
-- Per-table grants live with their CREATE TABLE (RULE SGR), never here — a grant
-- separated from the table it describes is how a role quietly keeps a privilege
-- after the table's callers change.

DO $$
DECLARE
    r text;
BEGIN
    -- No role is named after the runner: it holds zero datastore credentials and
    -- reaches PostgreSQL only through agentsfleetd. The worker role went with the
    -- worker process in the runtime split.
    -- `vault_runtime` and `billing_runtime` own the secret store and the wallet.
    -- api_runtime is a MEMBER of each (below) but holds no privilege of its own
    -- on those tables, so reaching either is a deliberate, greppable elevation
    -- rather than an ambient capability.
    FOREACH r IN ARRAY ARRAY[
        'db_migrator',
        'api_runtime',
        'memory_runtime',
        'vault_runtime',
        'billing_runtime',
        'ops_readonly_human',
        'ops_readonly_fleet'
    ]
    LOOP
        IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = r) THEN
            EXECUTE format('CREATE ROLE %I NOLOGIN', r);
        END IF;
    END LOOP;
END
$$;

REVOKE CREATE ON SCHEMA public FROM PUBLIC;

-- db_migrator: full Data Definition Language (DDL) authority, control plane
-- only. It retains authority on `vault` and `billing` as well, so a rebuild from
-- empty can re-author those tables.
GRANT ALL ON SCHEMA public, core, fleet, billing, vault, audit, memory TO db_migrator;

-- Runtime roles reach data, never DDL. USAGE on a schema is only the right to
-- name what is inside it; the table grants that make it useful live with each
-- CREATE TABLE, and for vault and billing they land on the owning role rather
-- than on api_runtime.
GRANT USAGE ON SCHEMA core, fleet, billing, vault, audit TO api_runtime;
GRANT USAGE ON SCHEMA memory TO memory_runtime;
GRANT USAGE ON SCHEMA vault TO vault_runtime;
GRANT USAGE ON SCHEMA billing TO billing_runtime;
GRANT USAGE ON SCHEMA audit TO ops_readonly_human, ops_readonly_fleet;

-- Membership, not privilege — and `INHERIT FALSE` is the whole reason that
-- sentence is true. A bare `GRANT role TO api_runtime` takes its inheritance
-- from api_runtime's own INHERIT attribute, which CREATE ROLE defaults to TRUE:
-- the member's privileges would then apply ambiently on every connection and no
-- handler would ever have to elevate to read a ciphertext or move a balance. The
-- boundary would exist in this comment and nowhere else. INHERIT FALSE leaves
-- the membership dormant until `SET ROLE` names it; SET TRUE is what permits
-- that (RULE CTX: the role boundary is the process boundary).
GRANT memory_runtime  TO api_runtime WITH INHERIT FALSE, SET TRUE;
GRANT vault_runtime   TO api_runtime WITH INHERIT FALSE, SET TRUE;
GRANT billing_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;

REVOKE CREATE ON SCHEMA public, core, fleet, billing, vault, audit, memory
FROM api_runtime, memory_runtime, vault_runtime, billing_runtime,
     ops_readonly_human, ops_readonly_fleet;

-- Future tables in the authoritative schemas carry no PUBLIC privilege. Stated
-- as a default privilege so it binds every table a later slot creates; the
-- migration runner owns those tables, so db_migrator is the grantor.
ALTER DEFAULT PRIVILEGES FOR ROLE db_migrator
    IN SCHEMA core, fleet, billing, vault, audit, memory
    REVOKE ALL ON TABLES FROM PUBLIC;

ALTER ROLE api_runtime SET search_path = core, fleet, billing, vault, audit, public;
ALTER ROLE memory_runtime SET search_path = memory, public;
ALTER ROLE vault_runtime SET search_path = vault, public;
ALTER ROLE billing_runtime SET search_path = billing, public;
ALTER ROLE ops_readonly_human SET search_path = audit, public;
ALTER ROLE ops_readonly_fleet SET search_path = audit, public;

-- Keep local superuser test connections deterministic when a role-specific URL
-- is not used (HANDLER_DB_TEST_URL in Continuous Integration and dev Docker).
DO $$
BEGIN
    EXECUTE format(
        'ALTER DATABASE %I SET search_path = core,fleet,billing,vault,audit,public',
        current_database()
    );
END
$$;
