-- `fleet_id` and `workspace_id` copy the event's scope, written once with it, so the detail
-- endpoint authorizes without joining the event. `sequence` orders calls within one run.
CREATE TABLE fleet.tool_calls (
    id                   UUID PRIMARY KEY,
    workspace_id         UUID    NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    fleet_id             UUID    NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    event_id             UUID    NOT NULL REFERENCES fleet.events(event_id) ON DELETE CASCADE,
    fencing_token        BIGINT  NOT NULL,
    sequence             BIGINT  NOT NULL,
    arguments            JSONB   NOT NULL,
    truncated_arguments  BOOLEAN NOT NULL,
    output               TEXT    NOT NULL,
    output_line_count    BIGINT  NOT NULL,
    truncated            BOOLEAN NOT NULL,
    byte_count           BIGINT  NOT NULL,
    created_at           TIMESTAMPTZ(3)  NOT NULL,
    updated_at           TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_tool_calls_event_id_fencing_token_sequence
        UNIQUE (event_id, fencing_token, sequence)
);

CREATE INDEX idx_tool_calls_workspace_id ON fleet.tool_calls (workspace_id);

CREATE INDEX idx_tool_calls_fleet_id ON fleet.tool_calls (fleet_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.tool_calls TO api_runtime;
