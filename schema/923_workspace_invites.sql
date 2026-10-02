-- Additive migration: an owner invites a person, by email, into their account.
--
-- One row per invite. It is pending until it is accepted, revoked, or its
-- expiry passes; acceptance writes the membership and stamps `accepted_at` in
-- one transaction (`afd_tenant::team::invitation`). A pending invite is the one kind the
-- partial indexes below cover, because the two questions asked on a hot path
-- are both about pending ones: "is this address already invited here" at
-- create, and "what is waiting for me" on every dashboard page an invitee
-- loads.
--
-- `email` is stored lowercased by the application, so both indexes match it
-- without a function. `role` and `email_status` stay TEXT with no CHECK: their
-- vocabularies live in application constants, and a value list here would
-- drift the moment one changed (RULE STS). `email_status`, `email_attempts`
-- and `email_sent_at` belong to the invite email; they are NULL, zero and
-- NULL until a send is attempted.
--
-- An expired invite is still pending as far as these indexes can tell: a
-- partial index predicate cannot compare against the clock. The create path
-- therefore revokes an expired pending invite for the same address in the same
-- transaction before it inserts the new one.
--
-- VERSION is 0.51.0, above the 0.30.0 anchor: this slot only adds. Idempotent
-- (IF NOT EXISTS) for a fresh bootstrap and a provisioned database alike.

CREATE TABLE IF NOT EXISTS core.invites (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_invites_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id       UUID NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    email           TEXT NOT NULL,
    role            TEXT NOT NULL,
    invited_by      UUID NOT NULL REFERENCES core.users(id) ON DELETE CASCADE,
    expires_at      BIGINT NOT NULL,
    accepted_at     BIGINT,
    accepted_by     UUID REFERENCES core.users(id) ON DELETE SET NULL,
    revoked_at      BIGINT,
    email_status    TEXT,
    email_attempts  INTEGER NOT NULL,
    email_sent_at   BIGINT,
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL
);

-- One pending invite per address per account, arbitrated by the index rather
-- than by a read before the write.
CREATE UNIQUE INDEX IF NOT EXISTS uq_invites_tenant_id_email_pending
    ON core.invites (tenant_id, email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;

-- What is waiting for an address, read on every page an invitee loads.
CREATE INDEX IF NOT EXISTS idx_invites_email_pending
    ON core.invites (email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;

-- Deleting an account cascades here by tenant, and deleting a user cascades
-- by inviter and clears the accepter. Accepted, revoked and expired rows stay,
-- so each delete finds its rows by these indexes rather than a table scan.
CREATE INDEX IF NOT EXISTS idx_invites_tenant_id
    ON core.invites (tenant_id);
CREATE INDEX IF NOT EXISTS idx_invites_invited_by
    ON core.invites (invited_by);
CREATE INDEX IF NOT EXISTS idx_invites_accepted_by
    ON core.invites (accepted_by)
    WHERE accepted_by IS NOT NULL;

-- api_runtime creates, lists, revokes and accepts invites.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.invites TO api_runtime;
