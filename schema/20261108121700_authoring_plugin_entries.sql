-- An account-owned Plugin card. Deleting it deletes its revisions; fleets installed from it keep
-- their own frozen copy. `head_revision_id` is the current revision (deferred foreign key).
CREATE TABLE authoring.plugin_entries (
    id                  UUID PRIMARY KEY,
    tenant_id           UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    name                TEXT   NOT NULL,
    description         TEXT   NOT NULL,
    credential_reasons  JSONB,
    head_revision_id    UUID,
    resource_version    BIGINT NOT NULL,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL
);

CREATE INDEX idx_plugin_entries_tenant_id_created_at_id
    ON authoring.plugin_entries (tenant_id, created_at DESC, id DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON authoring.plugin_entries TO api_runtime;

CREATE INDEX idx_plugin_entries_head_revision_id
    ON authoring.plugin_entries (head_revision_id) WHERE head_revision_id IS NOT NULL;
