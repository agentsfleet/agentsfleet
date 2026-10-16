-- A content-free authoring obligation survives private draft/message deletion.
-- IDs below deliberately have no draft/message foreign key: deleting text
-- must not erase an unresolved provider charge or permit another settlement.
CREATE TABLE IF NOT EXISTS fleet.authoring_attempts (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_attempts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    lease_id              UUID NOT NULL UNIQUE,
    runner_id             UUID NOT NULL REFERENCES fleet.runners(id),
    workspace_id          UUID NOT NULL,
    tenant_id             UUID NOT NULL,
    creator_id            UUID NOT NULL,
    draft_id              UUID NOT NULL,
    message_sequence      BIGINT NOT NULL,
    fence                 BIGINT NOT NULL,
    deletion_generation   BIGINT NOT NULL,
    capability_hash       TEXT NOT NULL,
    model_snapshot        JSONB NOT NULL,
    status                TEXT NOT NULL,
    lease_expires_at      BIGINT NOT NULL,
    deadline              BIGINT NOT NULL,
    reserved_nanos        BIGINT NOT NULL,
    settled_nanos         BIGINT NOT NULL,
    usage_unknown         BOOLEAN NOT NULL,
    calls                 INTEGER NOT NULL,
    input_tokens          BIGINT NOT NULL,
    output_tokens         BIGINT NOT NULL,
    activity_sequence     BIGINT NOT NULL,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    CONSTRAINT fk_authoring_attempts_workspace FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES core.workspaces(id, tenant_id) ON DELETE CASCADE,
    CONSTRAINT uq_authoring_attempts_message UNIQUE (draft_id, message_sequence)
);

-- Shared host capacity counts active, unexpired leases of both kinds.
CREATE INDEX IF NOT EXISTS idx_authoring_attempts_host_capacity
    ON fleet.authoring_attempts(runner_id, status, lease_expires_at);
-- Wallet admission sums unresolved reservations under the tenant wallet lock.
CREATE INDEX IF NOT EXISTS idx_authoring_attempts_wallet
    ON fleet.authoring_attempts(tenant_id, status);
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.authoring_attempts TO api_runtime;
