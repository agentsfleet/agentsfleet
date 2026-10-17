-- The fleet's resume bookmark, `{last_event_id, last_response}`, read when a run
-- resumes. Keyed by fleet (an identity exception: at most one per fleet). A table,
-- not a fleet column, because it is rewritten on every run and would otherwise
-- churn the fleet row every edit compares against.

CREATE TABLE fleet.sessions (
    fleet_id      UUID PRIMARY KEY REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    context_json  JSONB  NOT NULL,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.sessions TO api_runtime;
