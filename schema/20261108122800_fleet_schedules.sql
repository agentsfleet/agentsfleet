-- `provider_schedule_id` is the external scheduler's key; a signed fire resolves back through it.
-- `resource_version` is positive: zero would make "never synced" and "synced at version zero" one
-- state.
CREATE TABLE fleet.schedules (
    id                     UUID PRIMARY KEY,
    fleet_id               UUID    NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    source                 TEXT    NOT NULL,
    provider_schedule_id   TEXT    NOT NULL,
    cron_expression        TEXT    NOT NULL,
    timezone               TEXT    NOT NULL,
    message                TEXT    NOT NULL,
    once                   BOOLEAN NOT NULL,
    fire_at                TIMESTAMPTZ(3),
    recipe                 JSONB,
    desired_status         TEXT    NOT NULL,
    sync_status            TEXT    NOT NULL,
    resource_version       BIGINT  NOT NULL,
    sync_token             UUID,
    sync_lease_expires_at  TIMESTAMPTZ(3),
    last_error             TEXT,
    created_at             TIMESTAMPTZ(3)  NOT NULL,
    updated_at             TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT ck_schedules_resource_version_positive CHECK (resource_version > 0),
    CONSTRAINT uq_schedules_fleet_id_provider_schedule_id UNIQUE (fleet_id, provider_schedule_id)
);

CREATE INDEX idx_schedules_fleet_id_created_at ON fleet.schedules (fleet_id, created_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.schedules TO api_runtime;
