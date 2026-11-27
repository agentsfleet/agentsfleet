-- One capture plan, then one recoverable generation, of a turn's or a durable task's private
-- state: exactly one owner. Each owner draws `generation` under its own row's lock (turns'
-- next_checkpoint_generation, tasks' checkpoint_sequence). Allocation receipts live in
-- `thread.replay_receipts`.
CREATE TABLE runner.checkpoints (
    id UUID PRIMARY KEY,
    turn_id UUID REFERENCES thread.turns (id) ON DELETE CASCADE,
    durable_task_id UUID,
    fleet_id UUID NOT NULL REFERENCES fleet.fleets (id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    generation BIGINT NOT NULL CHECK (generation > 0),
    owner_run_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL CHECK (owner_fence > 0),
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    -- What sealed this generation's new objects and its root; a restore opens with it.
    cipher TEXT NOT NULL,
    -- NULL for an API-only turn: it saves no files.
    toolbox_digest TEXT,
    -- The sandbox the files were taken in, which a restore lands them only in: its engine and
    -- the SHA-256 of the image its guest agent booted from. NULL engine when no sandbox ran;
    -- NULL digest for the debug engine, whose guest agent runs in the runner's own process.
    engine TEXT,
    guest_agent_digest TEXT,
    page_ids UUID[] NOT NULL,
    upload_budget_bytes BIGINT NOT NULL CHECK (upload_budget_bytes >= 0),
    uploaded_bytes BIGINT NOT NULL CHECK (uploaded_bytes >= 0),
    logical_bytes BIGINT NOT NULL CHECK (logical_bytes >= 0),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    manifest_root TEXT,
    history_digest TEXT,
    -- The generation this one's pages are a delta against, of the same owner; NULL for a full
    -- manifest. No cascade: Postgres refuses to delete an ancestor a remaining delta names.
    parent_checkpoint_id UUID,
    -- Turn only: the head this plan expected, the folded inputs it covers, and its history
    -- root sealed under the workspace's checkpoint key with its own random nonce.
    expected_head_checkpoint_id UUID,
    covered_fold_ids UUID[],
    covered_fold_sequences BIGINT[],
    root_nonce BYTEA,
    root_sealed BYTEA,
    -- Task only, all or none: the generations it was planned under, its mode and repository,
    -- the input and effect cursors it covers; and its optional base and commit.
    durable_task_generation BIGINT CHECK (durable_task_generation > 0),
    stop_generation BIGINT CHECK (stop_generation >= 0),
    mode TEXT,
    repository TEXT,
    checkpoint_base_id UUID,
    commit_oid TEXT,
    covered_input_sequence BIGINT CHECK (covered_input_sequence >= 0),
    effect_watermark BIGINT CHECK (effect_watermark >= 0),
    expires_at TIMESTAMPTZ(3) NOT NULL,
    ready_at TIMESTAMPTZ(3),
    abandoned_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_checkpoints_durable_task FOREIGN KEY (durable_task_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT fk_checkpoints_base FOREIGN KEY (checkpoint_base_id, workspace_id, repository)
        REFERENCES runner.checkpoint_bases (id, workspace_id, repository),
    CONSTRAINT fk_checkpoints_parent FOREIGN KEY (parent_checkpoint_id, workspace_id)
        REFERENCES runner.checkpoints (id, workspace_id),
    CONSTRAINT uq_checkpoints_id_turn_id UNIQUE (id, turn_id),
    CONSTRAINT uq_checkpoints_durable_task_scope UNIQUE (id, durable_task_id, fleet_id, workspace_id),
    CONSTRAINT uq_checkpoints_id_workspace_id UNIQUE (id, workspace_id),
    CONSTRAINT uq_checkpoints_turn_id_generation UNIQUE (turn_id, generation),
    CONSTRAINT uq_checkpoints_durable_task_id_generation UNIQUE (durable_task_id, generation),
    CONSTRAINT ck_checkpoints_one_owner CHECK (num_nonnulls(turn_id, durable_task_id) = 1),
    CONSTRAINT ck_checkpoints_durable_task_columns CHECK (num_nulls(durable_task_id,
        durable_task_generation, stop_generation, mode, repository, covered_input_sequence,
        effect_watermark) IN (0, 7)),
    CONSTRAINT ck_checkpoints_durable_task_files CHECK (durable_task_id IS NULL OR (
        toolbox_digest IS NOT NULL AND cardinality(page_ids) > 0)),
    CONSTRAINT ck_checkpoints_api_only CHECK (toolbox_digest IS NOT NULL OR (
        cardinality(page_ids) = 0 AND logical_bytes = 0 AND entry_count = 0 AND upload_budget_bytes = 0)),
    CONSTRAINT ck_checkpoints_sandbox CHECK ((engine IS NULL OR toolbox_digest IS NOT NULL)
        AND (guest_agent_digest IS NULL OR engine IS NOT NULL)),
    CONSTRAINT ck_checkpoints_turn_sandbox CHECK (turn_id IS NULL OR (toolbox_digest IS NULL) = (engine IS NULL)),
    CONSTRAINT ck_checkpoints_uploaded_within_budget CHECK (uploaded_bytes <= upload_budget_bytes),
    CONSTRAINT ck_checkpoints_folds CHECK (
        (covered_fold_ids IS NULL) = (covered_fold_sequences IS NULL)
        AND cardinality(covered_fold_ids) = cardinality(covered_fold_sequences)),
    CONSTRAINT ck_checkpoints_ready_root CHECK (ready_at IS NULL OR manifest_root IS NOT NULL),
    CONSTRAINT ck_checkpoints_turn_ready CHECK (turn_id IS NULL OR ready_at IS NULL OR (
        history_digest IS NOT NULL AND covered_fold_ids IS NOT NULL
        AND root_nonce IS NOT NULL AND root_sealed IS NOT NULL)),
    CONSTRAINT ck_checkpoints_disposition CHECK (ready_at IS NULL OR abandoned_at IS NULL)
);

-- One capture in flight per owner.
CREATE UNIQUE INDEX uq_checkpoints_turn_id_pending ON runner.checkpoints (turn_id) WHERE ready_at IS NULL AND abandoned_at IS NULL;
CREATE UNIQUE INDEX uq_checkpoints_durable_task_id_pending ON runner.checkpoints (durable_task_id) WHERE ready_at IS NULL AND abandoned_at IS NULL;

CREATE INDEX idx_checkpoints_workspace_id_id ON runner.checkpoints (workspace_id, id);
CREATE INDEX idx_checkpoints_fleet_id ON runner.checkpoints (fleet_id);
CREATE INDEX idx_checkpoints_parent_checkpoint_id ON runner.checkpoints (parent_checkpoint_id) WHERE parent_checkpoint_id IS NOT NULL;
CREATE INDEX idx_checkpoints_checkpoint_base_id ON runner.checkpoints (checkpoint_base_id) WHERE checkpoint_base_id IS NOT NULL;
CREATE INDEX idx_checkpoints_turn_id_generation_ready ON runner.checkpoints (turn_id, generation) WHERE ready_at IS NOT NULL;
CREATE INDEX idx_checkpoints_expires_at_id_unready ON runner.checkpoints (expires_at, id) WHERE ready_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.checkpoints TO api_runtime;

ALTER TABLE thread.turns ADD CONSTRAINT fk_turns_head_checkpoint
    FOREIGN KEY (head_checkpoint_id, id) REFERENCES runner.checkpoints (id, turn_id)
    ON DELETE SET NULL (head_checkpoint_id) DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE fleet.durable_tasks ADD CONSTRAINT fk_durable_tasks_checkpoint
    FOREIGN KEY (checkpoint_id, id, fleet_id, workspace_id)
    REFERENCES runner.checkpoints (id, durable_task_id, fleet_id, workspace_id)
    ON DELETE SET NULL (checkpoint_id) DEFERRABLE INITIALLY DEFERRED;
