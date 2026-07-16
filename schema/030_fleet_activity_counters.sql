-- One activity-counter row per fleet, maintained at write time.
--
-- Why: the fleets list (the Live Wall's hot path) and the single-fleet detail
-- read need each fleet's lifetime event count and lifetime spend. Computing
-- those by aggregating core.fleet_events + core.fleet_execution_telemetry on
-- every read is O(all child rows in the workspace): measured at ~1.8s for a
-- mature workspace (100 fleets x 3000 events). Maintaining the counters at
-- write time makes the read an indexed one-to-one join, constant in history.
--
-- This migration owns the table as well as the triggers, so it upgrades an
-- existing database where migration 005 is already recorded as applied.

CREATE TABLE IF NOT EXISTS core.fleet_activity_counters (
    uid               UUID GENERATED ALWAYS AS (fleet_id) STORED PRIMARY KEY,
    CONSTRAINT ck_fleet_activity_counters_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    fleet_id           UUID NOT NULL UNIQUE REFERENCES core.fleets(id) ON DELETE CASCADE,
    events_processed   BIGINT NOT NULL DEFAULT 0,
    budget_used_nanos  BIGINT NOT NULL DEFAULT 0,
    created_at         BIGINT NOT NULL,
    updated_at         BIGINT NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_activity_counters TO api_runtime;

CREATE OR REPLACE FUNCTION core.fleet_events_bump_count() RETURNS trigger AS $$
BEGIN
    INSERT INTO core.fleet_activity_counters
        (fleet_id, events_processed, budget_used_nanos, created_at, updated_at)
    SELECT f.id, 1, 0, f.created_at, NEW.updated_at
      FROM core.fleets f WHERE f.id = NEW.fleet_id
    ON CONFLICT (fleet_id) DO UPDATE
       SET events_processed = core.fleet_activity_counters.events_processed + 1,
           updated_at = GREATEST(core.fleet_activity_counters.updated_at, EXCLUDED.updated_at);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_fleet_events_bump_count
    AFTER INSERT ON core.fleet_events
    FOR EACH ROW EXECUTE FUNCTION core.fleet_events_bump_count();

CREATE OR REPLACE FUNCTION core.fleet_telemetry_bump_budget() RETURNS trigger AS $$
DECLARE
    delta BIGINT;
BEGIN
    IF NEW.fleet_id !~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$' THEN
        RETURN NULL;
    END IF;
    IF TG_OP = 'INSERT' THEN
        delta := NEW.credit_deducted_nanos;
    ELSE
        delta := NEW.credit_deducted_nanos - OLD.credit_deducted_nanos;
    END IF;
    INSERT INTO core.fleet_activity_counters
        (fleet_id, events_processed, budget_used_nanos, created_at, updated_at)
    SELECT f.id, 0, delta, f.created_at, NEW.recorded_at
      FROM core.fleets f WHERE f.id = NEW.fleet_id::uuid
    ON CONFLICT (fleet_id) DO UPDATE
       SET budget_used_nanos = core.fleet_activity_counters.budget_used_nanos + EXCLUDED.budget_used_nanos,
           updated_at = GREATEST(core.fleet_activity_counters.updated_at, EXCLUDED.updated_at);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_fleet_telemetry_bump_budget
    AFTER INSERT OR UPDATE OF credit_deducted_nanos ON core.fleet_execution_telemetry
    FOR EACH ROW EXECUTE FUNCTION core.fleet_telemetry_bump_budget();

-- Backfill fleets and children that existed before migration 030.
INSERT INTO core.fleet_activity_counters
    (fleet_id, events_processed, budget_used_nanos, created_at, updated_at)
SELECT f.id,
       (SELECT COUNT(*) FROM core.fleet_events e WHERE e.fleet_id = f.id),
       COALESCE((SELECT SUM(t.credit_deducted_nanos)
                   FROM core.fleet_execution_telemetry t WHERE t.fleet_id = f.id::text), 0),
       f.created_at, f.updated_at
  FROM core.fleets f
ON CONFLICT (fleet_id) DO UPDATE
   SET events_processed = EXCLUDED.events_processed,
       budget_used_nanos = EXCLUDED.budget_used_nanos,
       updated_at = EXCLUDED.updated_at;
