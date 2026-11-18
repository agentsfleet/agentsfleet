-- One CLI device-login session. No plaintext secret: `ciphertext` is sealed to a CLI key the
-- daemon never holds, and `verification_code_hmac` is keyed by a pepper held only in memory.
-- Nothing deletes an expired row; every read refuses it by `expires_at`.
CREATE TABLE identity.device_sessions (
    id UUID PRIMARY KEY CONSTRAINT ck_device_sessions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    status TEXT NOT NULL,
    cli_public_key TEXT NOT NULL,
    token_name TEXT NOT NULL,
    dashboard_public_key TEXT,
    ciphertext TEXT,
    nonce TEXT,
    verification_code_hmac BYTEA,
    verification_attempt_count SMALLINT NOT NULL,
    approver_subject TEXT,
    aborted_reason TEXT,
    consumed_client_fingerprint BYTEA,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    -- Re-stamped on every transition.
    expires_at TIMESTAMPTZ(3) NOT NULL,
    -- The same-fingerprint replay window after consumption.
    replay_expires_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_device_sessions_approver_subject ON identity.device_sessions (approver_subject);

CREATE INDEX idx_device_sessions_expires_at ON identity.device_sessions (expires_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.device_sessions TO api_runtime;

-- The sealed ciphertext and code digest stay off the operator read paths.
REVOKE ALL ON identity.device_sessions FROM ops_readonly_human, ops_readonly_fleet;
