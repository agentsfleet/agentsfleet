-- Private root state and a matching full workspace publish in one transaction.
CREATE TABLE runner.turn_checkpoints (
    id UUID PRIMARY KEY,
    event_id UUID NOT NULL REFERENCES runner.turn_recovery (event_id) ON DELETE CASCADE,
    fleet_id UUID NOT NULL REFERENCES fleet.fleets (id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    setup_revision_id UUID NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    expected_turn_checkpoint_id UUID,
    owner_lease_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL CHECK (owner_fence > 0),
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    has_workspace BOOLEAN NOT NULL,
    toolbox_digest TEXT,
    page_count INTEGER NOT NULL CHECK (page_count >= 0),
    page_ids UUID[] NOT NULL,
    upload_budget_bytes BIGINT NOT NULL CHECK (upload_budget_bytes >= 0),
    uploaded_bytes BIGINT NOT NULL CHECK (uploaded_bytes >= 0 AND uploaded_bytes <= upload_budget_bytes),
    logical_bytes BIGINT NOT NULL CHECK (logical_bytes >= 0),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    manifest_root TEXT,
    history_digest TEXT,
    covered_fold_ids UUID[],
    covered_fold_sequences BIGINT[],
    root_dek_ciphertext BYTEA,
    root_dek_nonce BYTEA,
    root_dek_tag BYTEA,
    root_secret_nonce BYTEA,
    root_secret_ciphertext BYTEA,
    root_secret_tag BYTEA,
    root_kek_version INTEGER,
    expires_at TIMESTAMPTZ(3) NOT NULL,
    ready_at TIMESTAMPTZ(3),
    abandoned_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_turn_checkpoint_event UNIQUE (id, event_id),
    CONSTRAINT uq_turn_checkpoint_workspace UNIQUE (id, workspace_id),
    CONSTRAINT uq_turn_checkpoint_generation UNIQUE (event_id, generation),
    CONSTRAINT ck_turn_checkpoint_api_only CHECK (has_workspace OR (
        page_count = 0 AND logical_bytes = 0 AND entry_count = 0 AND upload_budget_bytes = 0)),
    CONSTRAINT ck_turn_checkpoint_pages CHECK (cardinality(page_ids) = page_count),
    CONSTRAINT ck_turn_checkpoint_workspace CHECK (has_workspace = (toolbox_digest IS NOT NULL)),
    CONSTRAINT ck_turn_checkpoint_folds CHECK (
        (covered_fold_ids IS NULL) = (covered_fold_sequences IS NULL)
        AND cardinality(covered_fold_ids) = cardinality(covered_fold_sequences)),
    CONSTRAINT ck_turn_checkpoint_ready CHECK (ready_at IS NULL OR (
        manifest_root IS NOT NULL AND history_digest IS NOT NULL AND covered_fold_ids IS NOT NULL
        AND root_dek_ciphertext IS NOT NULL AND root_dek_nonce IS NOT NULL AND root_dek_tag IS NOT NULL
        AND root_secret_nonce IS NOT NULL AND root_secret_ciphertext IS NOT NULL
        AND root_secret_tag IS NOT NULL AND root_kek_version IS NOT NULL)),
    CONSTRAINT ck_turn_checkpoint_disposition CHECK (ready_at IS NULL OR abandoned_at IS NULL)
);

CREATE UNIQUE INDEX uq_turn_checkpoint_pending ON runner.turn_checkpoints (event_id)
    WHERE ready_at IS NULL AND abandoned_at IS NULL;
CREATE INDEX idx_turn_checkpoints_workspace ON runner.turn_checkpoints (workspace_id, id);
CREATE INDEX idx_turn_checkpoints_fleet ON runner.turn_checkpoints (fleet_id);
CREATE INDEX idx_turn_checkpoints_ready ON runner.turn_checkpoints (event_id, generation)
    WHERE ready_at IS NOT NULL;
CREATE INDEX idx_turn_checkpoints_expiry ON runner.turn_checkpoints (expires_at, id)
    WHERE ready_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.turn_checkpoints TO api_runtime;

ALTER TABLE runner.turn_recovery ADD CONSTRAINT fk_turn_recovery_head
    FOREIGN KEY (head_turn_checkpoint_id, event_id) REFERENCES runner.turn_checkpoints (id, event_id)
    ON DELETE SET NULL (head_turn_checkpoint_id) DEFERRABLE INITIALLY DEFERRED;
