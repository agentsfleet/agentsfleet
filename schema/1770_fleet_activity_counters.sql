-- Dashboard totals per fleet, maintained by triggers (rule 5 exception: an
-- aggregate kept under the same trigger as its source). Keyed by fleet (an identity
-- exception). `event_count` follows `fleet.events` inserts here; `budget_used_nanos`
-- follows `billing.usage_lines` in slot 2040. The runtime only reads it.

CREATE TABLE fleet.activity_counters (
    fleet_id           UUID PRIMARY KEY REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    event_count        BIGINT NOT NULL,
    budget_used_nanos  BIGINT NOT NULL,
    created_at         TIMESTAMPTZ(3) NOT NULL,
    updated_at         TIMESTAMPTZ(3) NOT NULL
);

-- SECURITY DEFINER with a pinned search_path: api_runtime inserts events but holds
-- no write grant here, so the counter cannot be set by hand.
CREATE FUNCTION fleet.events_bump_count() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO pg_catalog, fleet
AS $$
BEGIN
    INSERT INTO fleet.activity_counters
        (fleet_id, event_count, budget_used_nanos, created_at, updated_at)
    SELECT f.id, 1, 0, f.created_at, NEW.updated_at
      FROM fleet.fleets f WHERE f.id = NEW.fleet_id
    ON CONFLICT (fleet_id) DO UPDATE
       SET event_count = fleet.activity_counters.event_count + 1,
           updated_at = GREATEST(fleet.activity_counters.updated_at, EXCLUDED.updated_at);
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_events_bump_count
    AFTER INSERT ON fleet.events
    FOR EACH ROW EXECUTE FUNCTION fleet.events_bump_count();

GRANT SELECT ON fleet.activity_counters TO api_runtime;
