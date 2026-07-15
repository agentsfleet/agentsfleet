-- Denormalized activity counters on core.fleets, maintained at write time.
--
-- Why: the fleets list (the Live Wall's hot path) and the single-fleet detail
-- read need each fleet's lifetime event count and lifetime spend. Computing
-- those by aggregating core.fleet_events + core.fleet_execution_telemetry on
-- every read is O(all child rows in the workspace): measured at ~1.8s for a
-- mature workspace (100 fleets x 3000 events). Maintaining the counters at
-- write time makes the read a plain column select (~0.3ms), constant in
-- history. The columns live on core.fleets (migration 005, DEFAULT 0); these
-- triggers keep them in step with the child tables.
--
-- Correctness:
--   * events_processed counts core.fleet_events rows, which are insert-only
--     (ON CONFLICT (fleet_id, event_id) DO NOTHING) and never individually
--     deleted -- a fleet purge cascades the whole fleet row away. So an AFTER
--     INSERT increment matches COUNT(*) exactly. A skipped ON CONFLICT insert
--     does not fire the AFTER INSERT trigger, so a replayed event never
--     double-counts.
--   * budget_used_nanos sums credit_deducted_nanos, which is set on the initial
--     INSERT and ACCUMULATED by the renewal upsert
--     (ON CONFLICT (event_id, charge_type) DO UPDATE SET credit_deducted_nanos
--        = existing + EXCLUDED). So the trigger fires on INSERT (add the whole
--     value) and on UPDATE OF credit_deducted_nanos (add only the delta), which
--     keeps the sum exact across both paths.
--
-- Both child tables are written only by api_runtime, which already holds UPDATE
-- on core.fleets (migration 005), so these invoker-rights triggers need no new
-- grant. The trigger functions match the plpgsql/invoker shape of the
-- append-only guards in migration 007. The counter columns are declared on the
-- table in migration 005 (pre-2.0 is teardown-rebuild, so 005 re-runs with them
-- on every migrate); this migration only wires the maintenance + backfill.

CREATE OR REPLACE FUNCTION core.fleet_events_bump_count() RETURNS trigger AS $$
BEGIN
    UPDATE core.fleets
       SET events_processed = events_processed + 1
     WHERE id = NEW.fleet_id;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_fleet_events_bump_count
    AFTER INSERT ON core.fleet_events
    FOR EACH ROW EXECUTE FUNCTION core.fleet_events_bump_count();

CREATE OR REPLACE FUNCTION core.fleet_telemetry_bump_budget() RETURNS trigger AS $$
BEGIN
    -- fleet_execution_telemetry.fleet_id is TEXT with no foreign key: production
    -- always stamps a real fleet UUID, but the column itself accepts any text,
    -- so guard the cast. A non-UUID id (or one naming no fleet) simply updates
    -- nothing -- the trigger never imposes a constraint the table doesn't. The
    -- format check keeps the UPDATE on the id primary-key index in the common
    -- path rather than falling back to an unindexed id::text comparison.
    IF NEW.fleet_id !~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$' THEN
        RETURN NULL;
    END IF;
    IF TG_OP = 'INSERT' THEN
        UPDATE core.fleets
           SET budget_used_nanos = budget_used_nanos + NEW.credit_deducted_nanos
         WHERE id = NEW.fleet_id::uuid;
    ELSE
        -- The renewal upsert accumulates credit; add only what changed.
        UPDATE core.fleets
           SET budget_used_nanos = budget_used_nanos
                 + (NEW.credit_deducted_nanos - OLD.credit_deducted_nanos)
         WHERE id = NEW.fleet_id::uuid;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_fleet_telemetry_bump_budget
    AFTER INSERT OR UPDATE OF credit_deducted_nanos ON core.fleet_execution_telemetry
    FOR EACH ROW EXECUTE FUNCTION core.fleet_telemetry_bump_budget();

-- One-time backfill for any fleet that already has children (a no-op on a fresh
-- teardown-rebuild, where the counters are born correct alongside the rows).
UPDATE core.fleets f
   SET events_processed = COALESCE(
           (SELECT COUNT(*) FROM core.fleet_events e WHERE e.fleet_id = f.id), 0),
       budget_used_nanos = COALESCE(
           (SELECT SUM(t.credit_deducted_nanos)
              FROM core.fleet_execution_telemetry t WHERE t.fleet_id = f.id::text), 0);
