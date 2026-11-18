-- A private setup chat, which exists before any fleet and may never become one. Source and
-- publication pointers clear on delete, so a card delete or fleet purge never blocks on a draft.
-- NULL `tenant_model_id` is the platform default; `expires_at` is 30 days after `closed_at`.
CREATE TABLE authoring.drafts (
    id                          UUID PRIMARY KEY,
    workspace_id                UUID   NOT NULL,
    tenant_id                   UUID   NOT NULL,
    user_id                     UUID   NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    title                       TEXT   NOT NULL,
    status                      TEXT   NOT NULL,
    tenant_model_id             UUID,
    head_candidate_id           UUID,
    source_type                 TEXT   NOT NULL,
    source_plugin_revision_id  UUID REFERENCES authoring.plugin_revisions(id) ON DELETE SET NULL,
    source_fleet_revision_id    UUID REFERENCES fleet.revisions(id) ON DELETE SET NULL,
    instance_name               TEXT,
    fleet_id                    UUID REFERENCES fleet.fleets(id) ON DELETE SET NULL,
    plugin_revision_id         UUID REFERENCES authoring.plugin_revisions(id) ON DELETE SET NULL,
    last_message_sequence       BIGINT NOT NULL,
    cancelled_through_sequence  BIGINT NOT NULL,
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
    ON authoring.drafts (workspace_id, user_id, updated_at DESC, id DESC);

CREATE INDEX idx_drafts_user_id ON authoring.drafts (user_id);

-- The offer probe asks whether a connected tenant runner's tenant has setup
-- work waiting (`afd_fleet/src/lease/sql/lease/select_waiting_work.sql`).
CREATE INDEX idx_drafts_tenant_id ON authoring.drafts (tenant_id);

CREATE INDEX idx_drafts_expires_at_id
    ON authoring.drafts (expires_at, id) WHERE expires_at IS NOT NULL;

CREATE INDEX idx_drafts_fleet_id ON authoring.drafts (fleet_id) WHERE fleet_id IS NOT NULL;

CREATE INDEX idx_drafts_source_plugin_revision_id
    ON authoring.drafts (source_plugin_revision_id) WHERE source_plugin_revision_id IS NOT NULL;

CREATE INDEX idx_drafts_source_fleet_revision_id
    ON authoring.drafts (source_fleet_revision_id) WHERE source_fleet_revision_id IS NOT NULL;

CREATE INDEX idx_drafts_plugin_revision_id
    ON authoring.drafts (plugin_revision_id) WHERE plugin_revision_id IS NOT NULL;

CREATE INDEX idx_drafts_tenant_model_id
    ON authoring.drafts (tenant_model_id) WHERE tenant_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON authoring.drafts TO api_runtime;

CREATE INDEX idx_drafts_head_candidate_id
    ON authoring.drafts (head_candidate_id) WHERE head_candidate_id IS NOT NULL;
