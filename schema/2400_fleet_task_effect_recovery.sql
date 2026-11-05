-- Expiring encrypted read-only custody survives deletion of private task data.
-- It cannot authorize another publication and is removed on resolution/expiry.
CREATE TABLE fleet.task_effect_recovery (
    effect_id UUID PRIMARY KEY REFERENCES fleet.task_effects (id),
    workspace_id UUID NOT NULL,
    dek_ciphertext BYTEA NOT NULL,
    dek_nonce BYTEA NOT NULL,
    dek_tag BYTEA NOT NULL,
    secret_nonce BYTEA NOT NULL,
    secret_ciphertext BYTEA NOT NULL,
    secret_tag BYTEA NOT NULL,
    kek_version INTEGER NOT NULL,
    credential_expires_at TIMESTAMPTZ(3) NOT NULL,
    due_at TIMESTAMPTZ(3) NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);
CREATE INDEX idx_effect_recovery_due ON fleet.task_effect_recovery (due_at, effect_id);
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_effect_recovery TO api_runtime;
