-- Cron or one-shot runs, synced to the external scheduler. A table, not a fleet
-- column, because a fleet has many schedules, each with its own sync state.
--
-- `provider_schedule_id` is the external scheduler's key, unique per fleet,
-- because a signed fire resolves back through it. `resource_version` is the
-- compare-and-swap counter the sync loop checks; zero would make "never synced"
-- and "synced at version zero" one state, hence the bound. `sync_token` and
-- `sync_lease_expires_at` are the sync worker's claim. `once` + `fire_at` describe
-- a one-shot run; `recipe` is the inherited run recipe.

CREATE TABLE fleet.schedules (
    id                     UUID PRIMARY KEY,
    CONSTRAINT ck_schedules_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id               UUID    NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    source                 TEXT    NOT NULL,
    provider_schedule_id   TEXT    NOT NULL,
    cron_expression        TEXT    NOT NULL,
    timezone               TEXT    NOT NULL,
    message                TEXT    NOT NULL,
    once                   BOOLEAN NOT NULL,
    fire_at                BIGINT,
    recipe                 JSONB,
    desired_status         TEXT    NOT NULL,
    sync_status            TEXT    NOT NULL,
    resource_version       BIGINT  NOT NULL,
    sync_token             UUID,
    sync_lease_expires_at  BIGINT,
    last_error             TEXT,
    created_at             BIGINT  NOT NULL,
    updated_at             BIGINT  NOT NULL,
    CONSTRAINT ck_schedules_resource_version_positive CHECK (resource_version > 0),
    CONSTRAINT uq_schedules_fleet_id_provider_schedule_id UNIQUE (fleet_id, provider_schedule_id)
);

-- The fleet's schedule list, oldest first.
CREATE INDEX idx_schedules_fleet_id_created_at ON fleet.schedules (fleet_id, created_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.schedules TO api_runtime;
