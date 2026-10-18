-- One runner lease running one chat turn, with money reserved and the model
-- pinned. A table, not a message column, because a turn can be retried and every
-- attempt carries its own reservation.
--
-- No foreign key to drafts, workspaces, tenants or users: settlements hang off an
-- attempt, and money outlives every deletion (deletion matrix). The identifiers
-- stay as opaque values; `dispatches.result` is scrubbed instead.
--
-- `runner_id` keeps its foreign key but clears to NULL when the runner's record
-- retires, the way `runner.fleet_slots.last_runner_id` does: a revoked runner's
-- row goes, the attempts it ran and the money they reserved stay. NULL means
-- the runner that ran it no longer exists, never that no runner did.
--
-- `fencing_token` is the lease epoch; `model_token_hash` and `model_snapshot` pin
-- the model the attempt may call. `lease_expires_at` and `expires_at` are the lease
-- and overall deadlines. `call_count` and `output_token_count` are aggregates kept
-- under the attempt lock (rule 5 exception), read by admission and renewal.
-- `reported_runtime_ms`, `usage_unknown` and `reserved_nanos` feed settlement;
-- `last_activity_sequence` orders activity reports.

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
    created_at              TIMESTAMPTZ(3)  NOT NULL,
    updated_at              TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_attempts_lease_id UNIQUE (lease_id),
    CONSTRAINT uq_attempts_draft_id_message_sequence UNIQUE (draft_id, message_sequence)
);

-- Host capacity: a runner's live attempts.
CREATE INDEX idx_attempts_runner_id_status_lease_expires_at
    ON authoring.attempts (runner_id, status, lease_expires_at);
-- Wallet admission: a tenant's open reservations.
CREATE INDEX idx_attempts_tenant_id_status ON authoring.attempts (tenant_id, status);
-- Erasure scrubs by workspace and by user.
CREATE INDEX idx_attempts_workspace_id ON authoring.attempts (workspace_id);
CREATE INDEX idx_attempts_user_id ON authoring.attempts (user_id);

GRANT SELECT, INSERT, UPDATE ON authoring.attempts TO api_runtime;
