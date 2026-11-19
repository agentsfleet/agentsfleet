-- Dashboard totals per fleet, kept by triggers on `thread.turns` (`event_count`, a turn whose
-- event opened) and `billing.usage_lines` (`budget_used_nanos`, nanodollars). The runtime only
-- reads it.
CREATE TABLE fleet.activity_counters (
    fleet_id           UUID PRIMARY KEY REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    event_count        BIGINT NOT NULL,
    budget_used_nanos  BIGINT NOT NULL,
    created_at         TIMESTAMPTZ(3) NOT NULL,
    updated_at         TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT ON fleet.activity_counters TO api_runtime;
