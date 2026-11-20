-- Private working state for a setup chat, one row per draft. `resource_version` mirrors the
-- document's generation and is written with it in the same statement.
CREATE TABLE memory.context_checkpoints (
    id                  UUID PRIMARY KEY,
    draft_id            UUID NOT NULL REFERENCES setup.drafts(id) ON DELETE CASCADE,
    resource_version    BIGINT NOT NULL,
    document            JSONB NOT NULL,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX idx_context_checkpoints_draft_id
    ON memory.context_checkpoints (draft_id);

GRANT USAGE ON SCHEMA memory TO api_runtime;

GRANT SELECT, INSERT, UPDATE, DELETE ON memory.context_checkpoints TO api_runtime;
