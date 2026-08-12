-- The composite role for the one statement whose grain is not a table.
--
-- The renew and settle paths are each a single fenced statement touching three
-- `fleet` tables plus the wallet and the ledger, and `SET ROLE` replaces the
-- privilege set rather than adding to it, so no per-table role can carry them.
-- Splitting the statement to fit per-table roles is not an option: its charge
-- arm and its cursor advance (`fleet.runner_affinity.last_metered_at`) must
-- commit together or a replayed renewal charges again.
--
-- `metering_runtime` is composed to exactly that statement's footprint:
--   - an INHERITING member of `billing_runtime`, so the wallet debit and the
--     ledger accumulate arm work under one SET ROLE;
--   - direct grants on exactly the three `fleet` tables the statement names,
--     each granted beside its CREATE TABLE (RULE SGR: schema/610, 630, 650).
-- Reach stays enumerable — the grant list IS the statement's table list.
--
-- Runs after schema/110 (billing_runtime must exist to be inherited) and before
-- any table slot (roles precede the grants that name them).
--
-- A database whose migration ledger predates the privilege split must be
-- rebuilt from empty: `billing_runtime` is created by an EDIT to
-- already-applied slot 110, which the once-only migrator never re-runs, so on
-- such a database this slot's membership grant below fails with
-- undefined_object (42704) and boot is refused. That loud failure is the
-- intended signal under the teardown-rebuild era — wipe and remigrate
-- (`make down && make up` locally); do not guard the grant, which would trade
-- the failure for a silently unenforced boundary.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'metering_runtime') THEN
        EXECUTE 'CREATE ROLE metering_runtime NOLOGIN';
    END IF;
END
$$;

-- INHERITING on purpose — the whole point of the composite. The unit test
-- asserting dormant memberships (schema_privilege_test.zig) asserts this one is
-- the deliberate exception.
GRANT billing_runtime TO metering_runtime WITH INHERIT TRUE;

-- The right to name what is inside `fleet`; the table grants that make it
-- useful live with each CREATE TABLE. USAGE on `billing` arrives through the
-- inherited membership above.
GRANT USAGE ON SCHEMA fleet TO metering_runtime;

-- Membership, not privilege — dormant until the metering path names it with
-- SET ROLE, exactly as schema/110 grants the other elevation roles.
GRANT metering_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;

REVOKE CREATE ON SCHEMA public, core, fleet, billing, vault, audit, memory
FROM metering_runtime;
