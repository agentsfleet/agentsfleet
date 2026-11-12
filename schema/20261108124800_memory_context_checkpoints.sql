-- Private conversation working state for a setup chat. Separate from semantic
-- memory and immutable revisions: each has a different owner, deletion
-- boundary and conditional-write rule.
-- The draft owns one row and removes it when it is deleted. resource_version
-- is the guarded projection of document.generation; the repository writes both
-- from one validated checkpoint in the same statement.
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
