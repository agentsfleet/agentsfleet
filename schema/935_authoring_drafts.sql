-- Creator-private setup state. The workspace supplies the billing tenant;
-- creator_id is the authenticated person, including a joined workspace member.
-- Application constants own lifecycle vocabulary and initial counter values.
CREATE TABLE IF NOT EXISTS core.authoring_drafts (
    id                    UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_drafts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id          UUID NOT NULL,
    tenant_id             UUID NOT NULL,
    creator_id            UUID NOT NULL REFERENCES core.users(id) ON DELETE CASCADE,
    operation_id          UUID NOT NULL,
    creation_digest       TEXT NOT NULL,
    title                 TEXT NOT NULL,
    state                 TEXT NOT NULL,
    authoring_model_ref   TEXT NOT NULL,
    runtime_model_ref     TEXT,
    current_candidate_id  UUID,
    candidate_revision    BIGINT NOT NULL,
    resource_version      BIGINT NOT NULL,
    last_sequence         BIGINT NOT NULL,
    cancelled_through     BIGINT NOT NULL,
    deletion_generation   BIGINT NOT NULL,
    linked_fleet_id        UUID REFERENCES core.fleets(id) ON DELETE SET NULL,
    publication           JSONB,
    closed_at             BIGINT,
    expires_at            BIGINT,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    CONSTRAINT fk_authoring_drafts_workspace FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES core.workspaces(id, tenant_id) ON DELETE CASCADE,
    CONSTRAINT uq_authoring_drafts_creation UNIQUE (workspace_id, creator_id, operation_id)
);

-- Creator lists use the same timestamp/UUID ordering as their bound cursor.
CREATE INDEX IF NOT EXISTS idx_authoring_drafts_creator_page
    ON core.authoring_drafts(workspace_id, creator_id, updated_at DESC, id DESC);
-- Expiry sweeper reads only closed drafts with a retention deadline.
CREATE INDEX IF NOT EXISTS idx_authoring_drafts_expiry
    ON core.authoring_drafts(expires_at, id) WHERE expires_at IS NOT NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON core.authoring_drafts TO api_runtime;
