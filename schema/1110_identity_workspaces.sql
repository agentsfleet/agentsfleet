-- The isolation unit inside a tenant. Fleets, drafts, secrets and installs are
-- workspace-scoped, so a workspace is a table rather than a tenant column.
--
-- Rows never change after creation (a rename is not offered), so no `updated_at`
-- and UPDATE is refused (rule 9).
-- `UNIQUE (id, tenant_id)` exists for composite references that must prove a
-- workspace belongs to the tenant named beside it.

CREATE TABLE identity.workspaces (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_workspaces_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id   UUID NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    name        TEXT,
    created_at  BIGINT NOT NULL,
    CONSTRAINT uq_workspaces_id_tenant_id UNIQUE (id, tenant_id)
);

-- The tenant's workspace list, oldest first, keyset-paged on (created_at, id).
CREATE INDEX idx_workspaces_tenant_id_created_at_id
    ON identity.workspaces (tenant_id, created_at, id);

-- A name is unique inside its tenant once given; unnamed workspaces never collide.
CREATE UNIQUE INDEX uq_workspaces_tenant_id_name
    ON identity.workspaces (tenant_id, name) WHERE name IS NOT NULL;

CREATE FUNCTION identity.workspaces_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'workspaces is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_workspaces_refuse_update
    BEFORE UPDATE ON identity.workspaces
    FOR EACH ROW EXECUTE FUNCTION identity.workspaces_refuse_update();

GRANT SELECT, INSERT, DELETE ON identity.workspaces TO api_runtime;
