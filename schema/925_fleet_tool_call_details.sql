-- Each tool call's full arguments and output, so "show all" under a call in
-- the thread has something to open after the run's sandbox is gone.
--
-- The event row keeps a call's first and last lines (`tool_calls`, slot 924).
-- This table keeps the rest: one row per call, posted by the runner under its
-- lease's fence and read back one call at a time. Bounded by the daemon before
-- the write, at most 64 KiB per field and 1 MiB per event and fence
-- (`afd_wire::tool_detail`); `byte_count` is what the budget sums, measured as
-- the runner measured it, because JSONB's own rendering of `arguments` is not.
--
-- Keyed by fence as well as call number: a reclaimed lease re-runs the event
-- and its runner numbers calls from 1 again, so two leases of one event both
-- have a call 1. The statement that settles the event deletes every other
-- fence's rows, so only the run whose answer stands keeps its records. The
-- unique key is what a retried post upserts on.
--
-- Rows go with their event: the composite key cascades from
-- `core.fleet_events`, which cascades from the fleet.
CREATE TABLE IF NOT EXISTS core.fleet_tool_call_details (
    id                  UUID    PRIMARY KEY,
    CONSTRAINT ck_fleet_tool_call_details_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id        UUID    NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    fleet_id            UUID    NOT NULL,
    event_id            TEXT    NOT NULL,
    fencing_token       BIGINT  NOT NULL,
    call_number         BIGINT  NOT NULL,
    arguments           JSONB   NOT NULL,
    truncated_arguments BOOLEAN NOT NULL,
    output              TEXT    NOT NULL,
    output_line_count   BIGINT  NOT NULL,
    truncated           BOOLEAN NOT NULL,
    byte_count          BIGINT  NOT NULL,
    created_at          BIGINT  NOT NULL,
    updated_at          BIGINT  NOT NULL,
    CONSTRAINT fk_fleet_tool_call_details_event
        FOREIGN KEY (fleet_id, event_id)
        REFERENCES core.fleet_events(fleet_id, event_id) ON DELETE CASCADE,
    CONSTRAINT uq_fleet_tool_call_details_call
        UNIQUE (fleet_id, event_id, fencing_token, call_number)
);

-- The workspace cascade walks this; every other read leads with the unique key.
CREATE INDEX IF NOT EXISTS idx_fleet_tool_call_details_workspace
    ON core.fleet_tool_call_details (workspace_id);

-- api_runtime upserts in the runner verb, deletes dead fences at settlement,
-- and serves the single-call read.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_tool_call_details TO api_runtime;
