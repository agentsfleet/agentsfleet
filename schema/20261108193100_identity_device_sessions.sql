-- One CLI device-login session (docs/AUTH_DEVICE_LOGIN.md), moved out of the
-- datastore by M244. A table rather than columns elsewhere: a session exists
-- before any credential does, and lives only SESSION_TTL (afd_tenant::session).
-- No plaintext secret: ciphertext is sealed to a CLI key the daemon never holds,
-- and verification_code_hmac is keyed by a pepper held only in process memory.
-- status: enforced by afd_tenant::session::SessionStatus, never a CHECK.
-- Readers: lock_session.sql (every column), select_session.sql (poll),
-- approve_session.sql and abort_owned_sessions.sql (status, approver_subject,
-- expires_at), and the expiry sweep (expires_at).
CREATE TABLE identity.device_sessions (
    id UUID PRIMARY KEY CONSTRAINT ck_device_sessions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    status TEXT NOT NULL,
    cli_public_key TEXT NOT NULL,
    token_name TEXT NOT NULL,
    dashboard_public_key TEXT,
    ciphertext TEXT,
    nonce TEXT,
    verification_code_hmac BYTEA,
    verification_attempts SMALLINT NOT NULL,
    approver_subject TEXT,
    aborted_reason TEXT,
    consumed_client_fingerprint BYTEA,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    -- A deadline: every transition re-stamps it, as the key's life was reset.
    expires_at TIMESTAMPTZ(3) NOT NULL,
    approved_at TIMESTAMPTZ(3),
    consumed_at TIMESTAMPTZ(3),
    -- A deadline: the same-fingerprint replay window after consumption.
    replay_expires_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_device_sessions_approver_subject ON identity.device_sessions (approver_subject);

CREATE INDEX idx_device_sessions_expires_at ON identity.device_sessions (expires_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.device_sessions TO api_runtime;

-- A session's sealed ciphertext and code digest stay away from the operator
-- read paths, as vault.secrets does.
REVOKE ALL ON identity.device_sessions FROM ops_readonly_human, ops_readonly_fleet;
