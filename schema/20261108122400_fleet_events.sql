-- One row per delivery into a fleet: what came in, what the fleet answered, and
-- what it cost in tokens and wall time. The activity feed, resume and runner
-- leases read it. `event_id` is the event's identity and the table's key, a
-- UUIDv7: an admitted message's own id, or one minted for an activity. A lease
-- and a tool call name the event by it, and it stays the event's name after the
-- row is gone.
--
-- `tool_calls` is the compact list the feed renders; full arguments and output
-- live in `fleet.tool_calls`. `resumes_event_id` names the event a resume
-- continues, read by the resume chain.
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
    tool_calls        JSONB,
    token_count       BIGINT,
    wall_ms           BIGINT,
    failure_reason    TEXT,
    failure_detail    TEXT,
    resumes_event_id  UUID,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    -- Public task progress survives reconnects separately from private recovery.
    task_activity JSONB NOT NULL
    DEFAULT jsonb_build_array(),
    task_activity_sequence BIGINT NOT NULL
    DEFAULT 0 CHECK (task_activity_sequence >= 0),
    task_activity_truncated BOOLEAN NOT NULL DEFAULT false,
    -- jsonb_array_length refuses non-arrays. The write's 120 KB payload bound
    -- leaves space for metadata inside the public history's 128 KiB ceiling.
    CONSTRAINT ck_events_task_activity_bound CHECK (jsonb_array_length(task_activity) <= 128
        AND octet_length(task_activity::text) <= 120000)
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

-- Setup editors filter receipt history before paging. Runtime bodies never
-- enter that walk, even when a fleet has many newer deliveries.
CREATE INDEX idx_events_fleet_id_event_type_created_at_event_id
    ON fleet.events (fleet_id, event_type, created_at DESC, event_id DESC);
