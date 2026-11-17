-- The invitee has no user row until they accept, so an invite is not a membership state.
-- `resolved_at` is NULL while pending and set once, on accept or revoke; `status` says which.
CREATE TABLE identity.invites (
    id                   UUID PRIMARY KEY,
    CONSTRAINT ck_invites_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id            UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    email                TEXT NOT NULL,
    role                 TEXT NOT NULL,
    status               TEXT NOT NULL,
    invited_by_user_id   UUID NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    accepted_by_user_id  UUID REFERENCES identity.users(id) ON DELETE SET NULL,
    expires_at           TIMESTAMPTZ(3) NOT NULL,
    resolved_at          TIMESTAMPTZ(3),
    email_status         TEXT,
    email_attempt_count  INTEGER NOT NULL,
    email_sent_at        TIMESTAMPTZ(3),
    created_at           TIMESTAMPTZ(3) NOT NULL,
    updated_at           TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX uq_invites_tenant_id_email_pending
    ON identity.invites (tenant_id, email) WHERE resolved_at IS NULL;

CREATE INDEX idx_invites_email_pending
    ON identity.invites (email) WHERE resolved_at IS NULL;

CREATE INDEX idx_invites_tenant_id ON identity.invites (tenant_id);

CREATE INDEX idx_invites_invited_by_user_id ON identity.invites (invited_by_user_id);

CREATE INDEX idx_invites_accepted_by_user_id
    ON identity.invites (accepted_by_user_id) WHERE accepted_by_user_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.invites TO api_runtime;
