-- `fleet_id` and `workspace_id` copy the event's scope, written once with it, so the detail
-- endpoint authorizes without joining the event. `sequence` orders calls within one run.
CREATE TABLE thread.items (
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
    CONSTRAINT uq_items_event_id_fencing_token_sequence
        UNIQUE (event_id, fencing_token, sequence)
);

CREATE INDEX idx_items_workspace_id ON thread.items (workspace_id);

CREATE INDEX idx_items_fleet_id ON thread.items (fleet_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON thread.items TO api_runtime;
