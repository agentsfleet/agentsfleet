-- Fleet-owned recurring schedules. agentsfleet stores intent and visible sync
-- state; the external scheduler owns timekeeping and calls the signed ingress
-- when a schedule is due. Provider credentials stay in the administrative vault,
-- never in this row (RULE VLT). `source`, `desired_status` and `sync_status`
-- vocabularies are app-enforced named constants, not SQL CHECKs (RULE STS).
--
-- `generation` is the optimistic-concurrency counter the sync loop compares
-- against, and `sync_token` plus `sync_lease_until` fence a single syncer so two
-- of them cannot both push the same schedule. Those three are why this table
-- carries its own identity rather than being keyed by `fleet_id`: a fleet holds
-- many schedules.

CREATE TABLE IF NOT EXISTS core.fleet_schedules (
    id               UUID   PRIMARY KEY,
    CONSTRAINT ck_fleet_schedules_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id         UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    source           TEXT   NOT NULL,
    source_key       TEXT   NOT NULL,
    cron_expression  TEXT   NOT NULL,
    timezone         TEXT   NOT NULL,
    message          TEXT   NOT NULL,
    desired_status   TEXT   NOT NULL,
    sync_status      TEXT   NOT NULL,
    generation       BIGINT NOT NULL,
    sync_token       UUID,
    sync_lease_until BIGINT,
    last_error       TEXT,
    created_at       BIGINT NOT NULL,
    updated_at       BIGINT NOT NULL,
    -- Numeric guard, not a value set: RULE STS bans frozen string vocabularies,
    -- and a positivity bound on a monotonic counter is not one. Generation zero
    -- would make "never synced" and "synced at generation zero" the same state.
    CONSTRAINT ck_fleet_schedules_generation_positive CHECK (generation > 0),
    -- The external scheduler's key is what a signed fire resolves back to, so it
    -- is unique per fleet. Holds a different value from `id` rather than
    -- duplicating it.
    CONSTRAINT uq_fleet_schedules_fleet_id_source_key UNIQUE (fleet_id, source_key)
);

-- Reader: the fleet's schedule list, filtered by fleet and ordered oldest-first
-- so the displayed order is stable across pages. The unique constraint above
-- leads with `fleet_id` and so serves the filter and the cascade, but stops
-- before `created_at` — this index carries the sort.
CREATE INDEX IF NOT EXISTS idx_fleet_schedules_fleet_id_created_at
    ON core.fleet_schedules (fleet_id, created_at);

-- api_runtime owns the schedule lifecycle and resolves signed fires to a fleet.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_schedules TO api_runtime;
