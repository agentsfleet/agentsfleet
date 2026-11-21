-- Private working state for one conversation, one row per thread: a setup draft's chat or a
-- fleet's. Each run of the thread reads it at its start and saves the summary its compaction
-- writes, fenced by the run. `resource_version` mirrors the document's generation and is
-- written with it in the same statement. It goes with its thread, which goes with its owner.
CREATE TABLE memory.context_checkpoints (
    id                  UUID PRIMARY KEY,
    thread_id           UUID NOT NULL REFERENCES thread.threads(id) ON DELETE CASCADE,
    resource_version    BIGINT NOT NULL,
    document            JSONB NOT NULL,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX idx_context_checkpoints_thread_id
    ON memory.context_checkpoints (thread_id);

GRANT USAGE ON SCHEMA memory TO api_runtime;

GRANT SELECT, INSERT, UPDATE, DELETE ON memory.context_checkpoints TO api_runtime;
