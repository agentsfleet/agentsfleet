-- One private setup chat. A table, not a fleet state, because a chat exists before
-- any fleet and may never become one.
--
-- Source and publication are separate facts: `source_type` plus one of the
-- `source_*` pointers record where the chat started; `library_revision_id` records
-- what it published and `fleet_id` the fleet it saved. All four clear on delete,
-- so deleting a card or purging a fleet never blocks on, or deletes, a draft.
-- `tenant_model_id` NULL is the platform default; the composite reference refuses
-- another tenant's model. `head_candidate_id` closes a cycle (slot 1920).
--
-- `last_message_sequence` and `cancelled_through_sequence` order and cancel turns;
-- `resource_version` guards edits. Closing stamps `closed_at` and `expires_at =
-- closed_at + 30 days`, both returned by the API; the sweeper deletes past
-- `expires_at` (deletion matrix: Draft).

CREATE TABLE authoring.drafts (
    id                          UUID PRIMARY KEY,
    CONSTRAINT ck_drafts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id                UUID   NOT NULL,
    tenant_id                   UUID   NOT NULL,
    user_id                     UUID   NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    title                       TEXT   NOT NULL,
    status                      TEXT   NOT NULL,
    tenant_model_id             UUID,
    head_candidate_id           UUID,
    source_type                 TEXT   NOT NULL,
    source_library_revision_id  UUID REFERENCES authoring.library_revisions(id) ON DELETE SET NULL,
    source_fleet_revision_id    UUID REFERENCES fleet.revisions(id) ON DELETE SET NULL,
    instance_name               TEXT,
    fleet_id                    UUID REFERENCES fleet.fleets(id) ON DELETE SET NULL,
    library_revision_id         UUID REFERENCES authoring.library_revisions(id) ON DELETE SET NULL,
    last_message_sequence       BIGINT NOT NULL,
    cancelled_through_sequence  BIGINT NOT NULL,
    resource_version            BIGINT NOT NULL,
    closed_at                   BIGINT,
    expires_at                  BIGINT,
    created_at                  BIGINT NOT NULL,
    updated_at                  BIGINT NOT NULL,
    CONSTRAINT fk_drafts_workspace_tenant
        FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES identity.workspaces (id, tenant_id) ON DELETE CASCADE,
    CONSTRAINT fk_drafts_tenant_model
        FOREIGN KEY (tenant_model_id, tenant_id)
        REFERENCES model.tenant_models (id, tenant_id) ON DELETE SET NULL (tenant_model_id)
);

-- The user's chat list, most recently touched first.
CREATE INDEX idx_drafts_workspace_id_user_id_updated_at_id
    ON authoring.drafts (workspace_id, user_id, updated_at DESC, id DESC);
CREATE INDEX idx_drafts_user_id ON authoring.drafts (user_id);
-- The retention sweep.
CREATE INDEX idx_drafts_expires_at_id
    ON authoring.drafts (expires_at, id) WHERE expires_at IS NOT NULL;
-- Clearing references on delete and purge.
CREATE INDEX idx_drafts_fleet_id ON authoring.drafts (fleet_id) WHERE fleet_id IS NOT NULL;
CREATE INDEX idx_drafts_source_library_revision_id
    ON authoring.drafts (source_library_revision_id) WHERE source_library_revision_id IS NOT NULL;
CREATE INDEX idx_drafts_source_fleet_revision_id
    ON authoring.drafts (source_fleet_revision_id) WHERE source_fleet_revision_id IS NOT NULL;
CREATE INDEX idx_drafts_library_revision_id
    ON authoring.drafts (library_revision_id) WHERE library_revision_id IS NOT NULL;
CREATE INDEX idx_drafts_tenant_model_id
    ON authoring.drafts (tenant_model_id) WHERE tenant_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON authoring.drafts TO api_runtime;
