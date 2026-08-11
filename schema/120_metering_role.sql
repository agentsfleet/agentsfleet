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
--   - direct grants on exactly the three `fleet` tables the statement names,
--     each granted beside its CREATE TABLE (RULE SGR: schema/610, 630, 650);
--   - direct grants on the two `billing` tables it names, and only the verbs
--     it issues: SELECT + UPDATE on the wallet, SELECT + INSERT + UPDATE on
--     the ledger (schema/700, schema/710).
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

-- The right to name what is inside `fleet` and `billing`; the table grants
-- that make it useful live with each CREATE TABLE (RULE SGR): the three fleet
-- tables in schema/610, 630, 650, the wallet in schema/700, the ledger in
-- schema/710.
--
-- No membership in `billing_runtime`. An inheriting membership was the first
-- shape and it was wider than the statement: it carried INSERT and DELETE on
-- `billing.tenant_wallet`, neither of which either fenced statement issues (it
-- reads the wallet and updates it; the row is created by the signup starter
-- grant under `billing_runtime` and erased by the tenant cascade). Composing
-- the reach from direct grants is what makes the claim above literally true —
-- the grant list IS the statement's table list, with nothing arriving sideways.
GRANT USAGE ON SCHEMA fleet, billing TO metering_runtime;

-- Revoked explicitly, not merely absent above. Roles and their memberships are
-- CLUSTER-level: they outlive `DROP DATABASE`, so a cluster that ever applied
-- the earlier inheriting-membership form still carries it — the composite would
-- keep INSERT and DELETE on the wallet through inheritance while this file
-- claims its reach is only what it grants directly. Revoking makes the slot
-- converge to the declared posture from either starting state. A membership
-- that was never granted revokes as a notice, not an error.
REVOKE billing_runtime FROM metering_runtime;

-- Membership, not privilege — dormant until the metering path names it with
-- SET ROLE, exactly as schema/110 grants the other elevation roles.
GRANT metering_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;

REVOKE CREATE ON SCHEMA public, core, fleet, billing, vault, audit, memory
FROM metering_runtime;
