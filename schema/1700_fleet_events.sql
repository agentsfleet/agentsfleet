-- One row per delivery into a fleet: what came in, what the fleet answered, and
-- what it cost in tokens and wall time. The activity feed, resume and runner
-- leases read it. `event_id` is the producer's identifier, unique per fleet.
--
-- `id` is minted by Postgres (`uuidv7()`), not by the writer: an insert names
-- the event by `(fleet_id, event_id)` and reads its row id back with
-- `RETURNING id`, so the row a tool call or a lease points at exists before
-- anything names it.
--
-- `tool_calls` is the compact list the feed renders; full arguments and output
-- live in `fleet.tool_calls`. `resumes_event_id` names the event a resume
-- continues, read by the resume chain.

CREATE TABLE fleet.events (
    id                UUID PRIMARY KEY DEFAULT uuidv7(),
    CONSTRAINT ck_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id          UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id      UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    event_id          TEXT   NOT NULL,
    actor             TEXT   NOT NULL,
    event_type        TEXT   NOT NULL,
    status            TEXT   NOT NULL,
    request_json      JSONB  NOT NULL,
    response_text     TEXT,
    tool_calls        JSONB,
    token_count       BIGINT,
    wall_ms           BIGINT,
    failure_reason    TEXT,
    failure_detail    TEXT,
    resumes_event_id  TEXT,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_events_fleet_id_event_id UNIQUE (fleet_id, event_id)
);

-- The fleet's feed, newest first, and its per-actor filter.
CREATE INDEX idx_events_fleet_id_created_at_event_id
    ON fleet.events (fleet_id, created_at DESC, event_id DESC);
CREATE INDEX idx_events_fleet_id_actor_created_at_event_id
    ON fleet.events (fleet_id, actor, created_at DESC, event_id DESC);
-- The workspace-wide feed.
CREATE INDEX idx_events_workspace_id_created_at_event_id
    ON fleet.events (workspace_id, created_at DESC, event_id DESC);
CREATE INDEX idx_events_fleet_id_resumes_event_id
    ON fleet.events (fleet_id, resumes_event_id) WHERE resumes_event_id IS NOT NULL;

-- A fleet lives in exactly one workspace, so the planner must not multiply the two
-- selectivities when a query filters on both.
CREATE STATISTICS fleet.stx_events_workspace_id_fleet_id (dependencies)
    ON fleet_id, workspace_id FROM fleet.events;

GRANT SELECT, INSERT, UPDATE ON fleet.events TO api_runtime;
