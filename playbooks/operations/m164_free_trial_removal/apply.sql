-- Bring an EXISTING database up to the schema the free-trial removal ships.
--
-- Why this file exists at all. VERSION is pre-2.0.0, so the schema slots are the
-- source of truth and a change is made by editing the slot that creates the
-- object: `schema/700_tenant_wallet.sql` loses a column, `schema/820_memory_entries.sql`
-- gains a REFERENCES clause. Both take effect on a teardown-and-rebuild, because
-- every slot guards itself with CREATE TABLE IF NOT EXISTS. A database that is
-- NOT being rebuilt therefore never sees either change — this script is the
-- hand-applied equivalent, for the dev database, until the next rebuild.
--
-- This is an operational artifact, deliberately NOT a numbered slot under
-- schema/. Adding it there would make it run twice on a rebuilt database and
-- would contradict the Schema Table Removal Guard's pre-2.0.0 path.
--
-- Safe to re-run: every statement is guarded, and the whole thing is one
-- transaction, so a failure leaves the database exactly as it was.
--
--   psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f apply.sql
--
-- Run `verify.sql` in the same directory afterwards; it prints PASS/FAIL rows
-- rather than requiring the operator to interpret psql output.

\set ON_ERROR_STOP on

-- ── Preflight ───────────────────────────────────────────────────────────────
--
-- The foreign key below VALIDATES every existing row. If this database has
-- memory rows whose fleet is already gone — exactly the leak the edge exists to
-- close — the ADD CONSTRAINT fails and the transaction rolls back. That is the
-- correct outcome, but the operator should see the count BEFORE the failure
-- rather than decode a constraint-violation message.
--
-- A non-zero count here is not a reason to skip the migration. It is a decision:
-- those rows are unreachable by every sweep in the product, so deleting them is
-- what erasure was always supposed to have done. Delete them with the statement
-- commented out below, then re-run.

SELECT
    count(*) AS orphaned_memory_rows,
    CASE WHEN count(*) = 0
         THEN 'preflight ok — no orphans, the constraint will validate cleanly'
         ELSE 'STOP — orphans present; read the note above before continuing'
    END AS verdict
FROM memory.memory_entries m
WHERE NOT EXISTS (SELECT 1 FROM core.fleets f WHERE f.id = m.fleet_id);

-- Uncomment ONLY after reading the preflight note. These rows belong to fleets
-- that no longer exist and are reachable by nothing else in the product.
--
-- DELETE FROM memory.memory_entries m
-- WHERE NOT EXISTS (SELECT 1 FROM core.fleets f WHERE f.id = m.fleet_id);

BEGIN;

-- ── 1. The free trial leaves the wallet (schema/700) ────────────────────────
--
-- Nothing reads this column any more: the predicate, the projection, and the
-- published response member are all deleted in the same milestone. Dropping it
-- last means an older binary rolled back onto this database still works, since
-- every remaining reader selects columns by name.

ALTER TABLE billing.tenant_wallet
    DROP COLUMN IF EXISTS free_trial_ends_at;

-- ── 2. Memory rows gain their parent (schema/820) ───────────────────────────
--
-- Named to match the inline CONSTRAINT in schema/820, so a database migrated by
-- hand and one rebuilt from the slots carry the same constraint name. Without
-- the name PostgreSQL would generate `memory_entries_fleet_id_fkey` here and the
-- two would silently differ.
--
-- DROP-then-ADD rather than a catalogue check: it is shorter, and it makes the
-- script idempotent against a partially-applied earlier run.

ALTER TABLE memory.memory_entries
    DROP CONSTRAINT IF EXISTS fk_memory_entries_fleet_id;

ALTER TABLE memory.memory_entries
    ADD CONSTRAINT fk_memory_entries_fleet_id
    FOREIGN KEY (fleet_id) REFERENCES core.fleets (id) ON DELETE CASCADE;

COMMIT;
