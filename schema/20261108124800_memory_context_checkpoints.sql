-- Private working state for a setup chat, one row per draft. `resource_version` mirrors the
-- document's generation and is written with it in the same statement.
CREATE TABLE memory.context_checkpoints (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_context_checkpoints_id_uuidv7
        CHECK (substring(id::text from 15 for 1) = '7'),
    draft_id            UUID NOT NULL REFERENCES authoring.drafts(id) ON DELETE CASCADE,
    resource_version    BIGINT NOT NULL,
    document            JSONB NOT NULL,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX idx_context_checkpoints_draft_id
    ON memory.context_checkpoints (draft_id);

GRANT USAGE ON SCHEMA memory TO api_runtime;

GRANT SELECT, INSERT, UPDATE, DELETE ON memory.context_checkpoints TO api_runtime;
