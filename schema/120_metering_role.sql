-- Substrate, part three: the one role whose grain is a STATEMENT rather than a
-- table.
--
-- Every other runtime role owns tables. This one owns a footprint. The settle
-- and renewal paths (`fleet/renewal_settle.zig`, `fleet/renewal.zig`) are each a
-- single fenced Common Table Expression spanning four tables across two schemas:
-- it locks the lease and the affinity slot `FOR UPDATE`, locks the wallet row,
-- computes the charge against the live balance, then writes the lease, the
-- affinity cursor, the wallet and the ledger — all in one statement.
--
-- `SET ROLE` REPLACES the active privilege set rather than adding to it, so
-- elevating to `billing_runtime` around that statement would drop the `fleet.*`
-- privileges the same statement needs half way through. The "one role per
-- table" default cannot express this, and the three ways out of that framing
-- are all worse than the problem:
--
--   * granting `billing_runtime` on the fleet tables spills the wallet role
--     across the control plane permanently, for every wallet path;
--   * splitting the statement destroys the fencing. The ledger upsert is a
--     deliberate accumulator (`credit_deducted_nanos = existing + EXCLUDED`,
--     schema/710), so replay ADDS; what makes a retry safe is that the charge
--     and the affinity cursor advance commit together. Split them and a crash
--     between the halves either loses a slice or double-charges;
--   * leaving the wallet on `api_runtime` is the boundary not existing.
--
-- So the role is composed to the statement's own shape instead. It is a member
-- of `billing_runtime` — inheriting the wallet and ledger grants rather than
-- restating them — plus direct grants on exactly the three `fleet` tables the
-- statement touches, which live with those tables per RULE SGR. Its reach is
-- enumerable by construction: the grant list IS the statement's table list.
--
-- This is narrower than what runs today, not wider. `api_runtime` already holds
-- every one of these fleet grants on every connection; here they are reachable
-- only inside an elevated metering section, and `billing_runtime` stays pure for
-- the non-metering money paths. A wallet writer still has no path to a
-- ciphertext — `vault_runtime` is disjoint from all of it.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'metering_runtime'
    ) THEN
        -- INHERIT is stated rather than defaulted, and it is load-bearing: the
        -- billing membership below is what carries the wallet and the ledger
        -- into the fenced statement. NOINHERIT here would lose them mid-
        -- statement, and it would fail on the money path at runtime rather than
        -- at bootstrap.
        CREATE ROLE metering_runtime NOLOGIN INHERIT;
    END IF;
END
$$;

-- Inherited, not re-granted: `metering_runtime` reaches `billing.tenant_wallet`
-- and `billing.usage_ledger` through membership, so the wallet's grants stay
-- stated once, in the slots that create those tables. INHERIT is the default and
-- is correct here — the privileges must be live the moment the role is assumed,
-- since the fenced statement cannot stop to elevate a second time.
GRANT billing_runtime TO metering_runtime;

-- Dormant until named, exactly as in schema/110. Without INHERIT FALSE the
-- composition above would hand `api_runtime` the wallet ambiently and undo the
-- boundary this slot exists to build.
GRANT metering_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;

GRANT USAGE ON SCHEMA fleet, billing TO metering_runtime;

REVOKE CREATE ON SCHEMA public, core, fleet, billing, vault, audit, memory
FROM metering_runtime;

-- No `ALTER ROLE … SET search_path` here, deliberately. Role settings live in
-- `pg_db_role_setting` and are applied at session start for the role that
-- AUTHENTICATES; `SET ROLE` does not re-apply them. This role is NOLOGIN and is
-- only ever assumed, so such a line would be inert — and a line that reads as a
-- safeguard while doing nothing is the failure this milestone is fixing
-- elsewhere. Every statement that runs here is schema-qualified (RULE NSQ),
-- which is the real guarantee.
