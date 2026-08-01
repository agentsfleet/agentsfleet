-- The fleet's checkpoint bookmark: one row per fleet, upserted after each event
-- delivery.
--
-- `context_json` is the conversation resume cursor, serialised as
-- {last_event_id, last_response}. It is NOT fleet memory — that lives in the
-- dedicated `memory` schema. Writing full conversation history or memory tool
-- output here is not what this column is for. On crash and restart the runner
-- reads this row to resume from the last event cursor.
--
-- `execution_id` and `execution_started_at` track the active runner session: set
-- at createExecution, cleared at destroyExecution and on claim (crash recovery).
-- NULL means the fleet is idle; non-NULL means it is actively executing an event.
--
-- Keyed by its parent, per the pattern stated in
-- `schema/430_tenant_model_selection.sql`: at most one bookmark exists per fleet,
-- the report path upserts it by `fleet_id`, and nothing addresses a bookmark by
-- an identity of its own. So `fleet_id` is both the foreign key and the primary
-- key, and the upsert's ON CONFLICT target IS that primary key. The retired
-- shape carried a generated identity column, a unique `id`, and a separate
-- `UNIQUE (fleet_id)` — three unique indexes over a row that is addressed one way.

CREATE TABLE IF NOT EXISTS core.fleet_sessions (
    fleet_id             UUID   PRIMARY KEY REFERENCES core.fleets(id) ON DELETE CASCADE,
    -- Structural DEFAULT: the empty object is the no-cursor identity, not a
    -- vocabulary value, so it is the same exception class as
    -- `core.fleets.required_tags` rather than the kind RULE STS bans.
    context_json         JSONB  NOT NULL DEFAULT '{}',
    checkpoint_at        BIGINT NOT NULL,
    execution_id         TEXT,
    execution_started_at BIGINT,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);

-- `checkpoint_at` keeps its domain name: it is the instant of the last
-- successful checkpoint, which is not when the row was written — a delivery that
-- fails to checkpoint still updates the row.

-- No uuidv7 CHECK: `fleet_id` is minted by `core.fleets`, whose slot carries the
-- version check, so repeating it would re-validate a value the parent guarantees.

-- No index. Every read and the upsert address this table by `fleet_id`, which is
-- the primary key, so the whole access path is already indexed.

-- api_runtime reads the session at lease issue, upserts it after each event in
-- the report path, and reads it for status display. No DELETE: rows leave with
-- their fleet through the cascade above.
GRANT SELECT, INSERT, UPDATE ON core.fleet_sessions TO api_runtime;
