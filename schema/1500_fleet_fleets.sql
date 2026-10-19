-- The deployed agent. Its setup is read from `fleet.revisions` only; this row keeps
-- no config copy (rule 5), and `head_revision_id` names the live revision (its
-- foreign key closes a cycle, so it lands in slot 1520).
--
-- Readers: `required_tags` by runner claim (GIN containment); `setup_stream_ready`
-- and `initial_start` by activation, which starts the first run once the setup
-- stream exists; the two `memory_*` flags by the memory tools; `resource_version`
-- by every compare-and-swap edit. Purging a fleet cascades everything it owns
-- except money (deletion matrix: Fleet).

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
-- The workspace's fleet list, newest first, and its status filter.
CREATE INDEX idx_fleets_workspace_id_created_at_id
    ON fleet.fleets (workspace_id, created_at DESC, id DESC);
CREATE INDEX idx_fleets_workspace_id_status_id ON fleet.fleets (workspace_id, status, id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.fleets TO api_runtime;
