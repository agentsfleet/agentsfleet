-- M1_001 §6.2: Zombie session checkpoint table.
-- One row per Zombie — upserted after each event delivery.
-- context_json: complete agent session state (conversation history, memory).
-- checkpoint_at: millis timestamp of last successful checkpoint.
-- On crash + restart, worker reads this row to resume from last known state.

CREATE TABLE IF NOT EXISTS core.zombie_sessions (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombie_sessions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id       UUID NOT NULL REFERENCES core.zombies(id),
    context_json    JSONB NOT NULL DEFAULT '{}',
    checkpoint_at   BIGINT NOT NULL,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,
    CONSTRAINT uq_zombie_sessions_zombie UNIQUE (zombie_id)
);

-- Worker reads session at claim; upserts (INSERT OR REPLACE by zombie_id) after each event.
-- API reads session for status display.
GRANT SELECT, INSERT, UPDATE ON core.zombie_sessions TO worker_runtime;
GRANT SELECT ON core.zombie_sessions TO api_runtime;
