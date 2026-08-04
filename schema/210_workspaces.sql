-- The workspace: the tenant's unit of isolation, and the scope almost every
-- authenticated read is filtered by.
--
-- `name` stays nullable. Most rows and every fixture insert omit it, and
-- uniqueness is per-tenant rather than global, so the partial unique index
-- below is what signup bootstrap relies on for collision retry via ON CONFLICT
-- — a NOT NULL column with a global unique would break both.
--
-- No `updated_at`: nothing updates a workspace row today. It gains one the
-- first time something does, per the mutable-table rule in the conventions.

CREATE TABLE IF NOT EXISTS core.workspaces (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_workspaces_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id   UUID NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    -- Human-readable, Heroku-style (`jolly-harbor-482`).
    name        TEXT,
    created_by  TEXT,
    created_at  BIGINT NOT NULL,
    -- Not a duplicate of the primary key but a superset of it, and it exists to
    -- be referenced: `core.fleets` carries a denormalised `tenant_id` and points
    -- at BOTH columns, so the database — rather than the fleet-create path —
    -- guarantees a fleet's tenant is really its workspace's tenant. PostgreSQL
    -- requires a unique constraint on the referenced columns for that, and `id`
    -- alone cannot serve a two-column reference.
    CONSTRAINT uq_workspaces_id_tenant_id UNIQUE (id, tenant_id)
);

-- The tenant's workspace list, cursor-paged by (created_at, id). Carrying the
-- tiebreak column means the keyset seek is one scan rather than a scan plus a
-- post-filter (RULE KYS); it also serves the plain tenant_id lookup as a prefix,
-- so no second index on tenant_id alone.
CREATE INDEX IF NOT EXISTS idx_workspaces_tenant_id_created_at_id
    ON core.workspaces (tenant_id, created_at, id);

-- Per-tenant name uniqueness. Partial, because a NULL name is the common case
-- and NULLs would otherwise not conflict anyway — stating it keeps the index
-- to the rows that can actually collide.
CREATE UNIQUE INDEX IF NOT EXISTS uq_workspaces_tenant_id_name
    ON core.workspaces (tenant_id, name) WHERE name IS NOT NULL;

-- api_runtime creates workspaces at signup, reads them on every authorized
-- request, and deletes them during account erasure.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.workspaces TO api_runtime;
