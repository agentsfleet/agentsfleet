-- Fleet-owned recurring schedules. agentsfleet stores intent and visible sync
-- state; QStash owns timekeeping and calls the signed ingress when due.
-- Provider credentials remain in the administrative vault, never in this row.
-- Source, desired_status, and sync_status values are app-enforced named enums.

CREATE TABLE IF NOT EXISTS core.fleet_schedules (
    uid                 UUID    PRIMARY KEY,
    CONSTRAINT ck_fleet_schedules_uid_uuidv7
        CHECK (substring(uid::text from 15 for 1) = '7'),
    fleet_id            UUID    NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    source              TEXT    NOT NULL,
    source_key          TEXT    NOT NULL,
    cron_expression     TEXT    NOT NULL,
    timezone            TEXT    NOT NULL,
    message             TEXT    NOT NULL,
    desired_status      TEXT    NOT NULL,
    sync_status         TEXT    NOT NULL,
    generation          BIGINT  NOT NULL,
    sync_token          UUID,
    sync_lease_until    BIGINT,
    last_error          TEXT,
    created_at          BIGINT  NOT NULL,
    updated_at          BIGINT  NOT NULL,
    CONSTRAINT ck_fleet_schedules_generation_positive CHECK (generation > 0),
    CONSTRAINT uq_fleet_schedules_fleet_source_key UNIQUE (fleet_id, source_key)
);

CREATE INDEX IF NOT EXISTS idx_fleet_schedules_fleet_created
    ON core.fleet_schedules (fleet_id, created_at);

-- api_runtime owns schedule lifecycle and resolves signed fires to a Fleet.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_schedules TO api_runtime;
