-- Zombie session CHECKPOINT BOOKMARK table.
-- One row per Zombie — upserted after each event delivery.
-- context_json: conversation resume cursor — serialized as {last_event_id, last_response}.
--   NOTE: This is NOT agent memory. Agent memory lives in the dedicated `memory`
--   schema. Writing full conversation history or memory tool outputs here is not
--   what this column is for.
-- checkpoint_at: millis timestamp of last successful checkpoint.
-- On crash + restart, worker reads this row to resume from the last event cursor.
--
-- execution_id + execution_started_at track the active executor session.
-- Set by the worker at createExecution, cleared at destroyExecution and on claim (crash recovery).
-- NULL means the zombie is idle. Non-NULL means it is actively executing an event.

CREATE TABLE IF NOT EXISTS core.zombie_sessions (
    id                   UUID   PRIMARY KEY,
    CONSTRAINT ck_zombie_sessions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id            UUID   NOT NULL REFERENCES core.zombies(id),
    context_json         JSONB  NOT NULL DEFAULT '{}',
    checkpoint_at        BIGINT NOT NULL,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    -- Active execution tracking (NULL = idle)
    execution_id         TEXT   NULL,
    execution_started_at BIGINT NULL,
    CONSTRAINT uq_zombie_sessions_zombie UNIQUE (zombie_id)
);

-- Worker reads session at claim, upserts (INSERT OR REPLACE by zombie_id) after each event.
-- API reads session for status display.
GRANT SELECT, INSERT, UPDATE ON core.zombie_sessions TO worker_runtime;
GRANT SELECT ON core.zombie_sessions TO api_runtime;
