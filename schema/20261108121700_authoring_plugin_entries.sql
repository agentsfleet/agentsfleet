-- An account-owned Plugin card. Its frozen revisions hold the reusable setup;
-- head_revision_id points at the current revision through a deferred foreign key.
--
-- `credential_reasons` is read by the install review, which explains why each
-- credential is asked for. `resource_version` is the compare-and-swap counter a
-- publish bumps. Deleting a card deletes its revisions (deletion matrix: Plugin
-- card); fleets installed from it keep their own frozen copy.
CREATE TABLE authoring.plugin_entries (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_plugin_entries_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id           UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    name                TEXT   NOT NULL,
    description         TEXT   NOT NULL,
    credential_reasons  JSONB,
    head_revision_id    UUID,
    resource_version    BIGINT NOT NULL,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL
);

-- The Plugin page, newest first, keyset-paged on (created_at, id).
CREATE INDEX idx_plugin_entries_tenant_id_created_at_id
    ON authoring.plugin_entries (tenant_id, created_at DESC, id DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON authoring.plugin_entries TO api_runtime;

CREATE INDEX idx_plugin_entries_head_revision_id
    ON authoring.plugin_entries (head_revision_id) WHERE head_revision_id IS NOT NULL;
