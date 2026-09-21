-- A charge can only ever land on the row of the fleet that incurred it.
--
-- `uq_usage_ledger_event_id_charge_type` (schema/710) arbitrates a charge by
-- `(event_id, charge_type)` alone. `event_id` is a LOGICAL id — the
-- `<millis>-<seq>` string a producer's admission mints — and nothing scopes
-- it to one fleet. schema/800:54 says so from the other side: it keys an
-- event by `(fleet_id, event_id)` because the id alone does not identify a
-- row.
--
-- So the ledger's correctness rests on an unwritten promise that the admission
-- sequence is global and never repeats across fleets. That promise is real and
-- it is not a billing guarantee anybody declared: it lives in a sequence's
-- implementation, not in the money table's key. The day it stops holding — a
-- per-fleet sequence, a second producer, a restored database — two fleets'
-- charges accumulate into ONE row and the `DO UPDATE` arms in `renew.rs` and
-- `report.rs` add one fleet's spend to the other's. No read would notice.
--
-- This slot moves the guarantee into the key. Destructive, and approved per
-- change (`docs/SCHEMA_CONVENTIONS.md`): Indy, Sep 20, 2026, recorded in the
-- Discovery of the spec that opened this slot, from finding A1 of
-- `docs/v2/reviews/identity-key-fk-shard-audit-2026-09-20.md`. No backfill:
-- the schema rebuilds from empty and nothing is deployed. The three
-- statements below run in the order written.

-- FIRST, because it is the only statement here that can fail on real data.
--
-- `fleet_id` is nullable today (schema/710 declared it a `SET NULL` reference;
-- schema/915 dropped the reference and left the column). Every writer already
-- binds it non-null, so this records a fact rather than changing behaviour.
--
-- It belongs to the key change, not beside it. PostgreSQL treats NULLs as
-- DISTINCT in a unique index, so a nullable column inside the arbiter is a
-- hole in it: a row with a null fleet escapes the constraint entirely and two
-- of them could never conflict.
--
-- Running first means a database holding a null row stops HERE, both
-- uniqueness constraints intact and nothing half-swapped. The row is refused,
-- not deleted: only a developer's database can hold one, and a migration that
-- deletes money rows is the wrong reflex even there. Clean it and rerun.
ALTER TABLE billing.usage_ledger
    ALTER COLUMN fleet_id SET NOT NULL;

-- SECOND. Dropped by name, and the name is the point.
--
-- schema/915 dropped this table's foreign key by catalogue lookup because
-- schema/710 declared that one inline and unnamed. This is the opposite case:
-- schema/710:74 spells the constraint explicitly, so the name is authored and
-- cannot drift. `IF EXISTS` only makes a second run idempotent.
ALTER TABLE billing.usage_ledger
    DROP CONSTRAINT IF EXISTS uq_usage_ledger_event_id_charge_type;

-- THIRD. The same arbiter, scoped to the fleet that pays.
--
-- The accumulate arms keep their meaning — a renewal for one fleet's event
-- still finds its own stage row and adds to it — and what changes is only
-- that a second fleet's identical event id now gets a row of its own instead
-- of merging into the first.
--
-- The fleet goes LAST deliberately. Leading with it was tried first, on the
-- reasoning that the index would then also serve a per-fleet spend read. It
-- does, and that is the problem:
-- `idx_usage_ledger_fleet_id_workspace_id_last_charged_at` is shaped for that
-- read, while a unique index leading with `fleet_id` merely LOOKS eligible —
-- so the planner took it and turned a ranged read into a scan of every row a
-- fleet was ever charged for, filtered afterwards:
--
--   Index Scan using uq_usage_ledger_fleet_id_event_id_charge_type
--     Index Cond: fleet_id = ... AND charge_type = ANY (...)
--     Filter: last_charged_at >= ... AND workspace_id = ...
--
-- That read is the budget drain, which runs every renewal tick of every live
-- run, so it grows with a fleet's lifetime spend. `integration_ledger_reads`
-- pins the plan and caught it. Uniqueness is identical whichever order the
-- columns sit in — the constraint arbitrates a SET — so the order is free to
-- be chosen for the one thing it does affect, which is which queries the
-- index tempts the planner into.
--
-- PostgreSQL has no `ADD CONSTRAINT IF NOT EXISTS`, so a re-runnable slot
-- either reads the catalogue or drops first. It drops first: the name is
-- authored here, so `IF EXISTS` needs no lookup to resolve it, and the
-- statement stays two lines of plain DDL a reader can check by eye.
--
-- The cost is real and is accepted: a SECOND run rebuilds the unique index
-- under an exclusive lock, where a catalogue guard would have been a no-op.
-- This schema rebuilds from empty and slots run once per database, so the
-- re-run is a developer's, not a deployment's.
ALTER TABLE billing.usage_ledger
    DROP CONSTRAINT IF EXISTS uq_usage_ledger_event_id_charge_type_fleet_id;

ALTER TABLE billing.usage_ledger
    ADD CONSTRAINT uq_usage_ledger_event_id_charge_type_fleet_id
    UNIQUE (event_id, charge_type, fleet_id);

-- No GRANT block (RULE SGR applies to CREATE TABLE): the constraint and the
-- column belong to a table schema/710 already granted.
COMMENT ON COLUMN billing.usage_ledger.fleet_id IS
    'The fleet this charge belongs to. Deliberately NOT a foreign key since slot 915: the purge would otherwise null it and strip a surviving charge of its attribution. The value may name a fleet that no longer exists, which is the point. NOT NULL and part of the accumulate arbiter since slot 916, so one fleet''s charges can never merge into another''s row.';
