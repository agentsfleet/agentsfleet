-- Verified repository bases can outlive the first task retaining them.
CREATE TABLE fleet.task_bases (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_base_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    repository TEXT NOT NULL,
    commit_oid TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    toolbox_digest TEXT NOT NULL,
    page_count INTEGER NOT NULL CHECK (page_count > 0),
    page_ids UUID[] NOT NULL,
    manifest_root TEXT,
    logical_bytes BIGINT NOT NULL CHECK (logical_bytes >= 0),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    retained_bytes BIGINT NOT NULL CHECK (retained_bytes >= 0),
    ready_at TIMESTAMPTZ(3),
    retired_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_task_base_scope UNIQUE (id, workspace_id, repository),
    CONSTRAINT uq_task_base_workspace UNIQUE (id, workspace_id),
    CONSTRAINT ck_task_base_root CHECK ((ready_at IS NULL) OR (manifest_root IS NOT NULL))
);

CREATE UNIQUE INDEX uq_task_base_live ON fleet.task_bases
    (workspace_id, repository, commit_oid, format_version, toolbox_digest) WHERE retired_at IS NULL;

CREATE INDEX idx_task_bases_retention ON fleet.task_bases (updated_at, id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_bases TO api_runtime;
