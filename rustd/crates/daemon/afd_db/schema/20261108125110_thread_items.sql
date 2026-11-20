-- One row per thing a run did. `fleet_id` and `workspace_id` copy the run's scope, written
-- once with it, so the detail endpoint authorizes without joining the turn. `sequence`
-- orders calls within one run. `turn_id` is the turn newest when the item's step began: the
-- run's own, or one its conversation folded in. `kind` and `status` hold the words
-- `af_wire::tool_detail::{ItemKind, ItemStatus}` spell; `changes` holds an edit's
-- `[{path, added, removed}]`, NULL for any other item, and is read by the tool call detail
-- (`afd_turns/src/view/history/select_tool_call.sql`).
CREATE TABLE thread.items (
    id                   UUID PRIMARY KEY,
    workspace_id         UUID    NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    fleet_id             UUID    NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    run_id               UUID    NOT NULL,
    turn_id              UUID    NOT NULL,
    fencing_token        BIGINT  NOT NULL,
    sequence             BIGINT  NOT NULL,
    kind                 TEXT    NOT NULL,
    tool                 TEXT,
    status               TEXT    NOT NULL,
    arguments            JSONB   NOT NULL,
    truncated_arguments  BOOLEAN NOT NULL,
    output               TEXT    NOT NULL,
    output_line_count    BIGINT  NOT NULL,
    truncated            BOOLEAN NOT NULL,
    byte_count           BIGINT  NOT NULL,
    changes              JSONB,
    created_at           TIMESTAMPTZ(3)  NOT NULL,
    updated_at           TIMESTAMPTZ(3)  NOT NULL,
    -- An item is the record of what one run did, so it goes with the run.
    CONSTRAINT fk_items_run_id FOREIGN KEY (run_id)
        REFERENCES thread.runs (id) ON DELETE CASCADE,
    -- And with the turn it names.
    CONSTRAINT fk_items_turn_id FOREIGN KEY (turn_id)
        REFERENCES thread.turns (id) ON DELETE CASCADE,
    -- A retried post rewrites the rows it already wrote and adds none.
    CONSTRAINT uq_items_run_id_sequence
        UNIQUE (run_id, sequence)
);

CREATE INDEX idx_items_workspace_id ON thread.items (workspace_id);

CREATE INDEX idx_items_fleet_id ON thread.items (fleet_id);

CREATE INDEX idx_items_turn_id ON thread.items (turn_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON thread.items TO api_runtime;
