-- A charge still names its fleet after that fleet is purged.
--
-- `billing.usage_ledger` already survives a fleet purge on purpose — schema/710
-- says why, and `afd_fleet_lifecycle/src/purge.rs` repeats it: a charge the
-- wallet was already debited for must outlive the fleet, or the reconciliation
-- between the ledger and the wallet is a lie. What did NOT survive was any way
-- to say WHICH fleet. `fleet_id` was a foreign key with `ON DELETE SET NULL`,
-- so the purge nulled the one column that answered it, and the dashboard fell
-- back to rendering "DELETED AGENT" against a real charge.
--
-- Two additive changes, and the purge itself is untouched: memory, approval
-- gates, integration grants and sessions are still destroyed exactly as before.
--
-- 1. The foreign key goes, the column stays. The dashboard derives a fleet's
--    callsign from the identifier alone (`ui/.../lib/fleets/identity.ts`), so
--    keeping the UUID restores the same label a live row shows, with no new
--    data and no read-path change. What is retained is an opaque identifier
--    pointing at a row that no longer exists — not customer content — which is
--    why this does not weaken the erasure the purge performs.
--
-- 2. `fleet_name` carries the operator-chosen name, captured at charge time.
--    A snapshot, not a mirror: rename a fleet and its old charges keep the old
--    name, because a ledger records what was true when the money moved.
--    `token_count_cached_input` (schema/710) set this precedent on this table —
--    "carried for auditability, not a query reader".
--
-- What this migration deliberately does NOT do: backfill. A fleet purged
-- before this slot took its identifier out of the database, and nothing here
-- can recover it. Those rows keep reading "DELETED AGENT" and the changelog
-- says so rather than leaving an operator to wonder why old rows differ.

-- Dropped by lookup rather than by name.
--
-- schema/710 declares the reference inline and unnamed, so PostgreSQL generated
-- the name. `DROP CONSTRAINT IF EXISTS usage_ledger_fleet_id_fkey` would be the
-- short spelling and the wrong one: if the generated name is ever anything
-- else, `IF EXISTS` swallows the miss and the migration reports success while
-- the foreign key — and the SET NULL that motivated this whole slot — is still
-- there. Finding the constraint by what it IS cannot fail that way.
--
-- The catalogue predicate is the definition of the thing being removed: a
-- FOREIGN KEY on `billing.usage_ledger` whose single constrained column is
-- `fleet_id`. `conkey` is a one-element array for this constraint, so the
-- `array_length` test is what keeps a future composite key out of the match.
DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    SELECT con.conname INTO constraint_name
    FROM pg_constraint con
    JOIN pg_class rel ON rel.oid = con.conrelid
    JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
    WHERE nsp.nspname = 'billing'
      AND rel.relname = 'usage_ledger'
      AND con.contype = 'f'
      AND array_length(con.conkey, 1) = 1
      AND con.conkey[1] = (
          SELECT att.attnum FROM pg_attribute att
          WHERE att.attrelid = rel.oid AND att.attname = 'fleet_id'
      );

    IF constraint_name IS NOT NULL THEN
        EXECUTE format(
            'ALTER TABLE billing.usage_ledger DROP CONSTRAINT %I',
            constraint_name
        );
    END IF;
END
$$;

-- Nullable with no default, and both properties are load-bearing.
--
-- Nullable because every row written before this slot has no name to carry, and
-- because a charge whose fleet row could not be read still has to be written —
-- a missing name must never cost a charge. No default, because there is no
-- value that would be true: an empty string or a placeholder would be a
-- fabricated name a reader could not distinguish from a real one. NULL says
-- "not captured", which is the only honest thing an old row can say.
--
-- No GRANT block (RULE SGR applies to CREATE TABLE): a new column on an
-- existing table inherits that table's privileges, which schema/710 already
-- granted to the roles that read and write here.
ALTER TABLE billing.usage_ledger
    ADD COLUMN IF NOT EXISTS fleet_name TEXT;

COMMENT ON COLUMN billing.usage_ledger.fleet_name IS
    'The fleet''s name as it stood when this charge was written. A snapshot, not a mirror: a later rename does not reach this row. NULL means the name was not captured — a charge written before slot 915, or one whose fleet row was unreadable.';

COMMENT ON COLUMN billing.usage_ledger.fleet_id IS
    'The fleet this charge belongs to. Deliberately NOT a foreign key since slot 915: the purge would otherwise null it and strip a surviving charge of its attribution. The value may name a fleet that no longer exists, which is the point.';
