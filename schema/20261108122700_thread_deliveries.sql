-- An answer owed to a destination until delivered or abandoned. A lane is one (provider,
-- workspace) pair ordered by `sequence`. Every claim writes a fresh `receipt` and every later
-- write matches it, so a lapsed holder's late write changes nothing.
CREATE TABLE thread.deliveries (
    id              UUID PRIMARY KEY,
    sequence        BIGINT GENERATED ALWAYS AS IDENTITY,
    fleet_id        UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id    UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    provider        TEXT   NOT NULL,
    destination     TEXT   NOT NULL,
    event_id        UUID   NOT NULL,
    answer          TEXT   NOT NULL,
    receipt         UUID NOT NULL,
    claim_expires_at TIMESTAMPTZ(3) NOT NULL,
    next_attempt_at TIMESTAMPTZ(3),
    delivered_at    TIMESTAMPTZ(3),
    attempt_count   BIGINT NOT NULL,
    abandoned_at    TIMESTAMPTZ(3),
    created_at      TIMESTAMPTZ(3) NOT NULL,
    updated_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_deliveries_fleet_id_event_id UNIQUE (fleet_id, event_id)
);

CREATE INDEX idx_deliveries_open_lane
    ON thread.deliveries (provider, workspace_id, sequence)
    WHERE delivered_at IS NULL AND abandoned_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON thread.deliveries TO api_runtime;
