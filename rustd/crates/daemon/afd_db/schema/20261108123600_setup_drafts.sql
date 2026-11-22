-- A private setup chat, which exists before any fleet and may never become one. Its proposals are
-- `fleet.revisions` rows it owns until one is installed. Source, head and fleet pointers clear on
-- delete, so a fleet purge never blocks on a draft.
-- NULL `tenant_model_id` is the platform default; `expires_at` is 30 days after `closed_at`.
CREATE TABLE setup.drafts (
    id                          UUID PRIMARY KEY,
    workspace_id                UUID   NOT NULL,
    tenant_id                   UUID   NOT NULL,
    user_id                     UUID   NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    title                       TEXT   NOT NULL,
    status                      TEXT   NOT NULL,
    tenant_model_id             UUID,
    -- Deferred, so a turn inserts its proposal and moves the pointer in one transaction.
    head_revision_id            UUID REFERENCES fleet.revisions(id) ON DELETE SET NULL
                                DEFERRABLE INITIALLY DEFERRED,
    source_type                 TEXT   NOT NULL,
    source_fleet_revision_id    UUID REFERENCES fleet.revisions(id) ON DELETE SET NULL,
    instance_name               TEXT,
    fleet_id                    UUID REFERENCES fleet.fleets(id) ON DELETE SET NULL,
    resource_version            BIGINT NOT NULL,
    closed_at                   TIMESTAMPTZ(3),
    expires_at                  TIMESTAMPTZ(3),
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    updated_at                  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_drafts_workspace_tenant
        FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES identity.workspaces (id, tenant_id) ON DELETE CASCADE,
    CONSTRAINT fk_drafts_tenant_model
        FOREIGN KEY (tenant_model_id, tenant_id)
        REFERENCES model.tenant_models (id, tenant_id) ON DELETE SET NULL (tenant_model_id)
);

CREATE INDEX idx_drafts_workspace_id_user_id_updated_at_id
    ON setup.drafts (workspace_id, user_id, updated_at DESC, id DESC);

CREATE INDEX idx_drafts_user_id ON setup.drafts (user_id);

-- The waiting-work probe asks whether a connected tenant runner's tenant has setup
-- work waiting (`afd_fleet/src/run/sql/run/select_waiting_work.sql`).
CREATE INDEX idx_drafts_tenant_id ON setup.drafts (tenant_id);

CREATE INDEX idx_drafts_fleet_id ON setup.drafts (fleet_id) WHERE fleet_id IS NOT NULL;

CREATE INDEX idx_drafts_source_fleet_revision_id
    ON setup.drafts (source_fleet_revision_id) WHERE source_fleet_revision_id IS NOT NULL;

CREATE INDEX idx_drafts_tenant_model_id
    ON setup.drafts (tenant_model_id) WHERE tenant_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON setup.drafts TO api_runtime;

CREATE INDEX idx_drafts_head_revision_id
    ON setup.drafts (head_revision_id) WHERE head_revision_id IS NOT NULL;

-- A draft's proposals go with it; an installed revision has left the draft for its fleet.
ALTER TABLE fleet.revisions
    ADD CONSTRAINT fk_revisions_draft FOREIGN KEY (draft_id) REFERENCES setup.drafts(id)
    ON DELETE CASCADE;
