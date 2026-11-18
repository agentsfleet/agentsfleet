-- One runner lease running one chat turn, with money reserved and the model pinned. No foreign
-- key to drafts, workspaces, tenants or users: usage lines name the attempt and money outlives
-- every deletion. NULL `runner_id` means the runner was removed, never that none ran it.
CREATE TABLE authoring.attempts (
    id                      UUID PRIMARY KEY,
    CONSTRAINT ck_attempts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    lease_id                UUID    NOT NULL,
    runner_id               UUID    REFERENCES runner.runners(id) ON DELETE SET NULL (runner_id),
    workspace_id            UUID    NOT NULL,
    tenant_id               UUID    NOT NULL,
    user_id                 UUID    NOT NULL,
    draft_id                UUID    NOT NULL,
    message_sequence        BIGINT  NOT NULL,
    fencing_token           BIGINT  NOT NULL,
    model_token_hash        TEXT    NOT NULL,
    model_snapshot          JSONB   NOT NULL,
    status                  TEXT    NOT NULL,
    lease_expires_at        TIMESTAMPTZ(3)  NOT NULL,
    expires_at              TIMESTAMPTZ(3)  NOT NULL,
    reserved_nanos          BIGINT  NOT NULL,
    usage_unknown           BOOLEAN NOT NULL,
    call_count              INTEGER NOT NULL,
    output_token_count      BIGINT  NOT NULL,
    reported_runtime_ms     BIGINT  NOT NULL,
    last_activity_sequence  BIGINT  NOT NULL,
    failure_reason          TEXT,
    -- NULL until settled; a replayed report answers with it after private text is gone.
    outcome                 TEXT,
    created_at              TIMESTAMPTZ(3)  NOT NULL,
    updated_at              TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_attempts_lease_id UNIQUE (lease_id),
    CONSTRAINT uq_attempts_draft_id_message_sequence UNIQUE (draft_id, message_sequence)
);

CREATE INDEX idx_attempts_runner_id_status_lease_expires_at
    ON authoring.attempts (runner_id, status, lease_expires_at);

-- The expression matches the lapse sweep's own ORDER BY, so the scan reads in order and stops
-- at its limit.
CREATE INDEX idx_attempts_status_deadline_id
    ON authoring.attempts (status, LEAST(lease_expires_at, expires_at), id);

CREATE INDEX idx_attempts_tenant_id_status ON authoring.attempts (tenant_id, status);

CREATE INDEX idx_attempts_workspace_id ON authoring.attempts (workspace_id);

GRANT SELECT, INSERT, UPDATE ON authoring.attempts TO api_runtime;
