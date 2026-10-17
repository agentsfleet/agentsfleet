-- An emailed offer to join a tenant. A table, not a membership state, because the
-- invitee has no user row until they accept.
--
-- `resolved_at` is the NULL marker the pending indexes read (rule 3): NULL while
-- pending, stamped on accept or revoke, so resolution happens once — the resolving
-- UPDATE is guarded on `resolved_at IS NULL`. `status` says which way it resolved.
-- `email_status`, `email_attempt_count` and `email_sent_at` are read by the invite
-- mailer's retry loop and the invite list.

CREATE TABLE identity.invites (
    id                   UUID PRIMARY KEY,
    CONSTRAINT ck_invites_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id            UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    email                TEXT NOT NULL,
    role                 TEXT NOT NULL,
    status               TEXT NOT NULL,
    invited_by_user_id   UUID NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    accepted_by_user_id  UUID REFERENCES identity.users(id) ON DELETE SET NULL,
    expires_at           BIGINT NOT NULL,
    resolved_at          BIGINT,
    email_status         TEXT,
    email_attempt_count  INTEGER NOT NULL,
    email_sent_at        BIGINT,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL
);

-- One pending invite per address per tenant.
CREATE UNIQUE INDEX uq_invites_tenant_id_email_pending
    ON identity.invites (tenant_id, email) WHERE resolved_at IS NULL;

-- "What is waiting for me", read at sign-in by address.
CREATE INDEX idx_invites_email_pending
    ON identity.invites (email) WHERE resolved_at IS NULL;

CREATE INDEX idx_invites_tenant_id ON identity.invites (tenant_id);
CREATE INDEX idx_invites_invited_by_user_id ON identity.invites (invited_by_user_id);
CREATE INDEX idx_invites_accepted_by_user_id
    ON identity.invites (accepted_by_user_id) WHERE accepted_by_user_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON identity.invites TO api_runtime;
