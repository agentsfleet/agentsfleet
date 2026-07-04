-- Fleet session CHECKPOINT BOOKMARK table.
-- One row per Fleet — upserted after each event delivery.
-- context_json: conversation resume cursor — serialized as {last_event_id, last_response}.
--   NOTE: This is NOT fleet memory. Fleet memory lives in the dedicated `memory`
--   schema. Writing full conversation history or memory tool outputs here is not
--   what this column is for.
-- checkpoint_at: millis timestamp of last successful checkpoint.
-- On crash + restart, worker reads this row to resume from the last event cursor.
--
-- execution_id + execution_started_at track the active runner session.
-- Set by the worker at createExecution, cleared at destroyExecution and on claim (crash recovery).
-- NULL means the fleet is idle. Non-NULL means it is actively executing an event.

CREATE TABLE IF NOT EXISTS core.fleet_sessions (
    uid                  UUID   GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_fleet_sessions_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id                   UUID   NOT NULL UNIQUE,
    fleet_id            UUID   NOT NULL REFERENCES core.fleets(id),
    context_json         JSONB  NOT NULL DEFAULT '{}',
    checkpoint_at        BIGINT NOT NULL,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    -- Active execution tracking (NULL = idle)
    execution_id         TEXT   NULL,
    execution_started_at BIGINT NULL,
    CONSTRAINT uq_fleet_sessions_fleet_id UNIQUE (fleet_id)
);

-- api_runtime reads session at lease issue, upserts (INSERT OR REPLACE by
-- fleet_id) after each event in the report path, and reads for status display.
GRANT SELECT, INSERT, UPDATE ON core.fleet_sessions TO api_runtime;
