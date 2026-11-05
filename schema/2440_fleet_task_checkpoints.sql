-- Plans are not recovery pointers until all immutable pages and objects verify.
CREATE TABLE fleet.task_checkpoints (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_checkpoint_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    task_generation BIGINT NOT NULL CHECK (task_generation > 0),
    accepted_revision_id UUID NOT NULL,
    stop_generation BIGINT NOT NULL CHECK (stop_generation >= 0),
    mode TEXT NOT NULL,
    repository TEXT NOT NULL,
    base_id UUID,
    commit_oid TEXT,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    toolbox_digest TEXT NOT NULL,
    page_count INTEGER NOT NULL CHECK (page_count > 0),
    page_ids UUID[] NOT NULL,
    upload_budget_bytes BIGINT NOT NULL CHECK (upload_budget_bytes >= 0),
    uploaded_bytes BIGINT NOT NULL CHECK (uploaded_bytes >= 0),
    logical_bytes BIGINT NOT NULL CHECK (logical_bytes >= 0),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    manifest_root TEXT,
    history_digest TEXT,
    covered_input_sequence BIGINT NOT NULL CHECK (covered_input_sequence >= 0),
    effect_watermark BIGINT NOT NULL CHECK (effect_watermark >= 0),
    owner_lease_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL,
    expires_at TIMESTAMPTZ(3) NOT NULL,
    transfer_deadline TIMESTAMPTZ(3) NOT NULL,
    ready_at TIMESTAMPTZ(3),
    abandoned_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_task_checkpoints_task FOREIGN KEY (task_id, fleet_id, workspace_id)
        REFERENCES fleet.tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_task_checkpoint_revision FOREIGN KEY (accepted_revision_id, fleet_id)
        REFERENCES fleet.revisions (id, fleet_id),
    CONSTRAINT fk_task_checkpoints_base FOREIGN KEY (base_id, workspace_id, repository)
        REFERENCES fleet.task_bases (id, workspace_id, repository),
    CONSTRAINT uq_task_checkpoint_scope UNIQUE (id, task_id, fleet_id, workspace_id),
    CONSTRAINT uq_task_checkpoint_workspace UNIQUE (id, workspace_id),
    CONSTRAINT uq_task_checkpoint_generation UNIQUE (task_id, generation),
    CONSTRAINT ck_task_checkpoint_root CHECK ((ready_at IS NULL) OR (manifest_root IS NOT NULL))
);
CREATE INDEX idx_task_checkpoints_base ON fleet.task_checkpoints (base_id) WHERE base_id IS NOT NULL;
CREATE UNIQUE INDEX uq_task_checkpoint_capture ON fleet.task_checkpoints (task_id)
    WHERE ready_at IS NULL AND abandoned_at IS NULL;
CREATE INDEX idx_task_checkpoints_expiry ON fleet.task_checkpoints ((COALESCE(abandoned_at, expires_at)), id)
    WHERE ready_at IS NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_checkpoints TO api_runtime;
