-- The deployed agent. Its setup lives only in `fleet.revisions`, and `head_revision_id` names
-- the live one. Purging a fleet cascades everything it owns except money.
CREATE TABLE fleet.fleets (
    id                          UUID PRIMARY KEY,
    CONSTRAINT ck_fleets_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    name                        TEXT    NOT NULL,
    status                      TEXT    NOT NULL,
    head_revision_id            UUID,
    required_tags               TEXT[]  NOT NULL,
    setup_stream_ready          BOOLEAN,
    initial_start               JSONB,
    memory_reads_workspace      BOOLEAN NOT NULL,
    memory_publishes_workspace  BOOLEAN NOT NULL,
    resource_version            BIGINT  NOT NULL,
    created_at                  TIMESTAMPTZ(3)  NOT NULL,
    updated_at                  TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_fleets_workspace_id_name UNIQUE (workspace_id, name),
    CONSTRAINT fk_fleets_workspace_tenant
        FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES identity.workspaces (id, tenant_id) ON DELETE CASCADE,
    -- For runner leases, which must prove the fleet's workspace and tenant.
    CONSTRAINT uq_fleets_id_workspace_id_tenant_id UNIQUE (id, workspace_id, tenant_id)
);

CREATE INDEX idx_fleets_required_tags_gin ON fleet.fleets USING gin (required_tags);

CREATE INDEX idx_fleets_workspace_id_created_at_id
    ON fleet.fleets (workspace_id, created_at DESC, id DESC);

CREATE INDEX idx_fleets_workspace_id_status_id ON fleet.fleets (workspace_id, status, id);

-- The offer probe asks whether a connected tenant runner's tenant has work
-- waiting (`afd_fleet/src/lease/sql/lease/select_waiting_work.sql`).
CREATE INDEX idx_fleets_tenant_id ON fleet.fleets (tenant_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.fleets TO api_runtime;

CREATE INDEX idx_fleets_head_revision_id
    ON fleet.fleets (head_revision_id) WHERE head_revision_id IS NOT NULL;
