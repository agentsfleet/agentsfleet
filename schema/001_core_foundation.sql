-- 001_core_foundation.sql
-- Core foundation: schema creation, tenants, and workspaces.
-- Split from the original monolithic 001_initial.sql.

-- Domain schemas: app data is segmented by bounded context.
CREATE SCHEMA IF NOT EXISTS core;
CREATE SCHEMA IF NOT EXISTS fleet;
CREATE SCHEMA IF NOT EXISTS billing;

-- Audit schema: migration bookkeeping + immutable operator audit
-- Tables (schema_migrations, schema_migration_failures) are created by the
-- migration runner in pool.zig before any SQL files execute.
CREATE SCHEMA IF NOT EXISTS audit;

CREATE TABLE IF NOT EXISTS core.tenants (
    uid          UUID GENERATED ALWAYS AS (tenant_id) STORED PRIMARY KEY,
    CONSTRAINT ck_tenants_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    tenant_id    UUID NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS core.workspaces (
    uid                       UUID GENERATED ALWAYS AS (workspace_id) STORED PRIMARY KEY,
    CONSTRAINT ck_workspaces_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    workspace_id              UUID NOT NULL UNIQUE,
    tenant_id                 UUID NOT NULL REFERENCES core.tenants(tenant_id),
    -- Human-readable workspace name (e.g. Heroku-style `jolly-harbor-482`).
    -- Nullable because most workspace rows and fixture INSERTs do not supply a
    -- name; uniqueness is enforced per-tenant via the partial index below, so
    -- signup bootstrap can rely on ON CONFLICT for collision retry.
    name                      TEXT,
    created_by                TEXT,
    created_at                BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workspaces_tenant ON core.workspaces(tenant_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_workspaces_tenant_name
    ON core.workspaces(tenant_id, name) WHERE name IS NOT NULL;
