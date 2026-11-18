-- The fleet's resume bookmark, rewritten on every run. It is kept off the fleet row so it does not
-- churn the row every edit compares against.
CREATE TABLE fleet.sessions (
    fleet_id      UUID PRIMARY KEY REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    context_json  JSONB  NOT NULL,
    created_at    TIMESTAMPTZ(3) NOT NULL,
    updated_at    TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.sessions TO api_runtime;
