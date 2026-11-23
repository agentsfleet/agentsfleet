-- A verified repository base a durable task's incremental checkpoints build on. It belongs to the
-- workspace, not to a task, so it outlives the task that first captured it. Its pages and object
-- refs live in `runner.checkpoint_pages` and `runner.checkpoint_object_refs`, owned by
-- `checkpoint_base_id`; it stays its own table because its lifecycle (shared, retired, kept by
-- age) is not a checkpoint's.
CREATE TABLE runner.checkpoint_bases (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    repository TEXT NOT NULL,
    commit_oid TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    toolbox_digest TEXT NOT NULL,
    page_ids UUID[] NOT NULL CHECK (cardinality(page_ids) > 0),
    manifest_root TEXT,
    logical_bytes BIGINT NOT NULL CHECK (logical_bytes >= 0),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    ready_at TIMESTAMPTZ(3),
    retired_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_checkpoint_bases_scope UNIQUE (id, workspace_id, repository),
    CONSTRAINT uq_checkpoint_bases_id_workspace_id UNIQUE (id, workspace_id),
    CONSTRAINT ck_checkpoint_bases_ready_root CHECK ((ready_at IS NULL) OR (manifest_root IS NOT NULL))
);

CREATE UNIQUE INDEX uq_checkpoint_bases_live ON runner.checkpoint_bases
    (workspace_id, repository, commit_oid, format_version, toolbox_digest) WHERE retired_at IS NULL;

CREATE INDEX idx_checkpoint_bases_retention ON runner.checkpoint_bases (updated_at, id);

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.checkpoint_bases TO api_runtime;
