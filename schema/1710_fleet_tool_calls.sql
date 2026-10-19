-- Full arguments and output of each tool call, behind the event feed's compact
-- list. A table, not an events column, because one event makes many calls and the
-- detail is read one call at a time.
--
-- `fleet_id` and `workspace_id` are scope copies (rule 5 exception): the detail
-- endpoint authorizes by workspace without joining the event. Both are written
-- once, from the event row, in the same statement. `sequence` is the call's
-- position within one lease (`fencing_token`).

CREATE TABLE fleet.tool_calls (
    id                   UUID PRIMARY KEY,
    CONSTRAINT ck_tool_calls_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id         UUID    NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    fleet_id             UUID    NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    fleet_event_id       UUID    NOT NULL REFERENCES fleet.events(id) ON DELETE CASCADE,
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
    CONSTRAINT uq_tool_calls_fleet_event_id_fencing_token_sequence
        UNIQUE (fleet_event_id, fencing_token, sequence)
);

CREATE INDEX idx_tool_calls_workspace_id ON fleet.tool_calls (workspace_id);
CREATE INDEX idx_tool_calls_fleet_id ON fleet.tool_calls (fleet_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.tool_calls TO api_runtime;
