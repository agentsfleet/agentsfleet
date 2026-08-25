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
    -- `memory_runtime` is the one elevation role today: api_runtime is a MEMBER
    -- of it but holds no privilege of its own on `memory`, so reaching that
    -- schema is a deliberate, greppable elevation rather than an ambient
    -- capability. Giving the secret store and the wallet the same treatment is
    -- its own milestone.
    FOREACH r IN ARRAY ARRAY[
        'db_migrator',
        'api_runtime',
        'memory_runtime',
        'ops_readonly_human',
        'ops_readonly_fleet'
    ]
    LOOP
        -- Caught, not checked. `IF NOT EXISTS (SELECT … pg_roles) THEN CREATE`
        -- reads like a guard and is not one: a role is CLUSTER-global while the
        -- advisory lock this migration runs under is per-DATABASE, so two
        -- databases on one cluster migrating at the same time both see the role
        -- missing, both issue CREATE, and the loser gets
        -- `23505 duplicate key value violates unique constraint
        -- "pg_authid_rolname_index"` — a migration failure with a message about
        -- a system index, which is not a thing anyone debugs quickly.
        --
        -- Two databases on one cluster is the normal case, not a corner: every
        -- test in the Rust suite creates its own database against the shared
        -- lane Postgres, and a staging and a production database on one managed
        -- instance are the same shape. Attempting the CREATE and forgiving the
        -- collision has no window between the check and the act.
        BEGIN
            EXECUTE format('CREATE ROLE %I NOLOGIN', r);
        EXCEPTION
            WHEN duplicate_object THEN
                -- Somebody else created it between our attempt and theirs. The
                -- role exists with the attributes this statement asks for,
                -- which is the whole postcondition.
                NULL;
        END;
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
-- CREATE TABLE.
GRANT USAGE ON SCHEMA core, fleet, billing, vault, audit TO api_runtime;
GRANT USAGE ON SCHEMA memory TO memory_runtime;
GRANT USAGE ON SCHEMA audit TO ops_readonly_human, ops_readonly_fleet;

-- Membership, not privilege — and `INHERIT FALSE` is the whole reason that
-- sentence is true. A bare `GRANT role TO api_runtime` takes its inheritance
-- from api_runtime's own INHERIT attribute, which CREATE ROLE defaults to TRUE:
-- the member's privileges would then apply ambiently on every connection and
-- nothing would ever have to elevate. INHERIT FALSE leaves the membership
-- dormant until `SET ROLE` names it; SET TRUE is what permits that
-- (RULE CTX: the role boundary is the process boundary).
--
-- Only `memory_runtime` today. Giving the secret store and the wallet the same
-- treatment is deferred to its own milestone — see the notes in schema/300 and
-- schema/700 on why the revoke and the elevation cannot ship apart.
GRANT memory_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;

REVOKE CREATE ON SCHEMA public, core, fleet, billing, vault, audit, memory
FROM api_runtime, memory_runtime,
     ops_readonly_human, ops_readonly_fleet;

-- Future tables in the authoritative schemas carry no PUBLIC privilege. Stated
-- as a default privilege so it binds every table a later slot creates, rather
-- than as `REVOKE … ON ALL TABLES`, which runs here against the empty set and
-- would miss everything created afterwards.
--
-- Deliberately NOT `FOR ROLE db_migrator`, though db_migrator holds the DDL
-- authority. Two reasons, both load-bearing:
--
--   1. Default privileges attach to tables the NAMED role creates. Nothing in
--      the migration path ever runs `SET ROLE db_migrator` — the runner connects
--      and creates every table as whatever role the environment handed it — so
--      naming db_migrator here binds to a grantor that creates nothing. It was a
--      no-op wherever it succeeded.
--   2. `FOR ROLE x` requires INHERITED membership in x. PostgreSQL 16 grants a
--      CREATEROLE creator only ADMIN OPTION (inherit_option = f), so a managed
--      non-superuser migrator is refused with 42501 — the whole migration fails.
--      A superuser bypasses the check, which is why every local and Continuous
--      Integration (CI) database accepted it and the managed ones could not.
--
-- Omitting the clause targets the role actually creating the tables, needs no
-- privilege the migrator lacks, and is the form the pre-rebuild schema used.
--
-- Note it stores no `pg_default_acl` row today: PostgreSQL grants PUBLIC no
-- privileges on new tables, so this revokes something never granted. It is
-- defence in depth against a later `ALTER DEFAULT PRIVILEGES … GRANT … TO
-- PUBLIC`, not an active grant removal — `make check-migrate-unprivileged`
-- pins the behaviour either way.
ALTER DEFAULT PRIVILEGES
    IN SCHEMA core, fleet, billing, vault, audit, memory
    REVOKE ALL ON TABLES FROM PUBLIC;

ALTER ROLE api_runtime SET search_path = core, fleet, billing, vault, audit, public;
ALTER ROLE memory_runtime SET search_path = memory, public;
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
