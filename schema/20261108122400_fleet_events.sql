-- One row per delivery into a fleet: what came in, what the fleet answered and what it cost.
-- `event_id` is an admitted message's own id or one minted for an activity, and stays the
-- event's name after the row is gone.
CREATE TABLE fleet.events (
    event_id          UUID PRIMARY KEY,
    CONSTRAINT ck_events_event_id_uuidv7 CHECK (substring(event_id::text from 15 for 1) = '7'),
    fleet_id          UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id      UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    actor             TEXT   NOT NULL,
    event_type        TEXT   NOT NULL,
    status            TEXT   NOT NULL,
    request_json      JSONB  NOT NULL,
    response_text     TEXT,
    -- A compact summary; full arguments and output live in `fleet.tool_calls`.
    tool_calls        JSONB,
    token_count       BIGINT,
    wall_ms           BIGINT,
    failure_reason    TEXT,
    failure_detail    TEXT,
    resumes_event_id  UUID,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    -- Public task progress, kept apart from private recovery so it survives reconnects.
    task_activity JSONB NOT NULL
    DEFAULT jsonb_build_array(),
    task_activity_sequence BIGINT NOT NULL
    DEFAULT 0 CHECK (task_activity_sequence >= 0),
    task_activity_truncated BOOLEAN NOT NULL DEFAULT false,
    -- jsonb_array_length also rejects non-arrays. 120000 bytes leaves room for metadata within
    -- the 128 KiB public history ceiling.
    CONSTRAINT ck_events_task_activity_bound CHECK (jsonb_array_length(task_activity) <= 128
        AND octet_length(task_activity::text) <= 120000)
);

CREATE INDEX idx_events_fleet_id_created_at_event_id
    ON fleet.events (fleet_id, created_at DESC, event_id DESC);

CREATE INDEX idx_events_fleet_id_actor_created_at_event_id
    ON fleet.events (fleet_id, actor, created_at DESC, event_id DESC);

CREATE INDEX idx_events_workspace_id_created_at_event_id
    ON fleet.events (workspace_id, created_at DESC, event_id DESC);

CREATE INDEX idx_events_fleet_id_resumes_event_id
    ON fleet.events (fleet_id, resumes_event_id) WHERE resumes_event_id IS NOT NULL;

-- A fleet lives in exactly one workspace, so the planner must not multiply the two
-- selectivities when a query filters on both.
CREATE STATISTICS fleet.stx_events_workspace_id_fleet_id (dependencies)
    ON fleet_id, workspace_id FROM fleet.events;

GRANT SELECT, INSERT, UPDATE ON fleet.events TO api_runtime;

CREATE INDEX idx_events_fleet_id_event_type_created_at_event_id
    ON fleet.events (fleet_id, event_type, created_at DESC, event_id DESC);
