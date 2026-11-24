-- The deployed agent: a name, a model, its network toggles, and what it owns by key. A person's
-- first message writes it `active` with its thread and first turn. Purging a fleet cascades
-- everything it owns except money. `model_ref` is the workspace model its runs resolve, read by
-- the run's issue and the fleet read; `http_egress` (`restricted` or `public`) and
-- `package_domains` feed the run's network policy (`afd_gate/src/policy/`), and a person flips
-- them. Their spellings are `af_fleet_runtime::config::HttpEgress`'s.
CREATE TABLE fleet.fleets (
    id                          UUID PRIMARY KEY,
    workspace_id                UUID    NOT NULL,
    tenant_id                   UUID    NOT NULL,
    name                        TEXT    NOT NULL,
    status                      TEXT    NOT NULL,
    model_ref                   TEXT    NOT NULL,
    http_egress                 TEXT    NOT NULL,
    package_domains             BOOLEAN NOT NULL,
    required_tags               TEXT[]  NOT NULL,
    memory_reads_workspace      BOOLEAN NOT NULL,
    memory_publishes_workspace  BOOLEAN NOT NULL,
    resource_version            BIGINT  NOT NULL,
    created_at                  TIMESTAMPTZ(3)  NOT NULL,
    updated_at                  TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_fleets_workspace_id_name UNIQUE (workspace_id, name),
    CONSTRAINT fk_fleets_workspace_tenant
        FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES identity.workspaces (id, tenant_id) ON DELETE CASCADE,
    -- For runner runs, which must prove the fleet's workspace and tenant.
    CONSTRAINT uq_fleets_id_workspace_id_tenant_id UNIQUE (id, workspace_id, tenant_id)
);

CREATE INDEX idx_fleets_required_tags_gin ON fleet.fleets USING gin (required_tags);

CREATE INDEX idx_fleets_workspace_id_created_at_id
    ON fleet.fleets (workspace_id, created_at DESC, id DESC);

CREATE INDEX idx_fleets_workspace_id_status_id ON fleet.fleets (workspace_id, status, id);

-- The waiting-work probe asks whether a connected tenant runner's tenant has work
-- waiting (`afd_fleet/src/run/sql/run/select_waiting_work.sql`).
CREATE INDEX idx_fleets_tenant_id ON fleet.fleets (tenant_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.fleets TO api_runtime;
