-- The triggers that maintain core.fleet_activity_counters.
--
-- Last slot on purpose: these attach to `core.fleet_events` (schema/800) and
-- `billing.usage_ledger` (schema/710), so both must exist first.
--
-- Both functions are SECURITY DEFINER, and that is what makes the per-schema
-- privilege split survive contact with the counters. A trigger function
-- otherwise executes as the role that wrote the triggering row, so the ledger
-- trigger would run as `billing_runtime` — a role deliberately granted nothing
-- outside `billing`, not even USAGE on `core`. The alternatives were to widen
-- that role until it could write a counter in `core`, which gives back exactly
-- the ambient reach the split exists to remove, or to make maintaining the
-- counter a capability of the caller rather than an invariant of the database.
-- Defining them instead means the counter is maintained by the schema owner
-- (the migration role, which creates these functions), no runtime role holds a
-- write grant on the counter table at all, and the reach a SECURITY DEFINER
-- function confers is bounded by these bodies.
--
-- `search_path` is pinned on both for the usual reason: a SECURITY DEFINER
-- function that resolves names through the caller's search_path can be pointed
-- at an attacker's objects. Pinning it makes every name here resolve to the
-- schemas this slot intends.
--
-- Both bodies carry NO value literals — no status vocabulary, no pattern match —
-- so nothing here can drift from an application constant (RULE STS).

-- One event inserted is one event counted.
CREATE OR REPLACE FUNCTION core.fleet_events_bump_count() RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, core
AS $$
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
$$;

CREATE TRIGGER trg_fleet_events_bump_count
    AFTER INSERT ON core.fleet_events
    FOR EACH ROW EXECUTE FUNCTION core.fleet_events_bump_count();

-- Spend is advanced by the DELTA, never by re-summing, because the ledger row
-- accumulates in place: a run that renews forty times updates one stage row
-- forty times, and each update must add only what it added. On INSERT the delta
-- is the whole charge; on UPDATE it is the difference, which is why the trigger
-- watches the amount column specifically rather than any update to the row.
--
-- The regular expression is gone. The retired body pattern-matched
-- `NEW.fleet_id` against a UUID shape and returned early if it failed, because
-- the ledger stored the identifier as bare TEXT with no reference — so the
-- trigger had to defend against a value that was never checked on the way in,
-- and cast it before use. The column is a UUID behind a foreign key now, so the
-- database has already answered both questions and the lookup below cannot miss.
CREATE OR REPLACE FUNCTION core.usage_ledger_bump_budget() RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, core, billing
AS $$
DECLARE
    delta BIGINT;
BEGIN
    IF TG_OP = 'INSERT' THEN
        delta := NEW.credit_deducted_nanos;
    ELSE
        delta := NEW.credit_deducted_nanos - OLD.credit_deducted_nanos;
    END IF;
    INSERT INTO core.fleet_activity_counters
        (fleet_id, events_processed, budget_used_nanos, created_at, updated_at)
    SELECT f.id, 0, delta, f.created_at, NEW.created_at
      FROM core.fleets f WHERE f.id = NEW.fleet_id
    ON CONFLICT (fleet_id) DO UPDATE
       SET budget_used_nanos = core.fleet_activity_counters.budget_used_nanos
                             + EXCLUDED.budget_used_nanos,
           updated_at = GREATEST(core.fleet_activity_counters.updated_at, EXCLUDED.updated_at);
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_usage_ledger_bump_budget
    AFTER INSERT OR UPDATE OF credit_deducted_nanos ON billing.usage_ledger
    FOR EACH ROW EXECUTE FUNCTION core.usage_ledger_bump_budget();

-- No backfill. The retired slot carried one, because it landed on a populated
-- database and had to count history that predated the triggers. A database
-- bootstrapped from empty has none: the first event and the first charge each
-- create the counter row through the ON CONFLICT arms above.
