-- 001_core_foundation.sql
-- Core foundation: schema creation, tenants, and workspaces.
-- Split from the original monolithic 001_initial.sql.

-- Domain schemas: app data is segmented by bounded context.
CREATE SCHEMA IF NOT EXISTS core;
CREATE SCHEMA IF NOT EXISTS agent;
CREATE SCHEMA IF NOT EXISTS billing;

-- Audit schema: migration bookkeeping + immutable operator audit
-- Tables (schema_migrations, schema_migration_failures) are created by the
-- migration runner in pool.zig before any SQL files execute.
CREATE SCHEMA IF NOT EXISTS audit;

CREATE TABLE IF NOT EXISTS core.tenants (
    tenant_id    UUID PRIMARY KEY,
    name         TEXT NOT NULL,
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS core.workspaces (
    workspace_id              UUID PRIMARY KEY,
    tenant_id                 UUID NOT NULL REFERENCES core.tenants(tenant_id),
    -- Human-readable workspace name (e.g. Heroku-style `jolly-harbor-482`).
    -- Nullable because most workspace rows and fixture INSERTs do not supply a
    -- name; uniqueness is enforced per-tenant via the partial index below, so
    -- signup bootstrap can rely on ON CONFLICT for collision retry.
    name                      TEXT,
    -- default_branch was required when workspace creation was 1:1 with a
    -- GitHub repo. The binding step is moving out of creation; new
    -- workspaces leave it NULL until a repo is attached. Fixtures still
    -- INSERT NULL for parity.
    default_branch            TEXT,
    paused                    BOOLEAN NOT NULL DEFAULT FALSE,
    paused_reason             TEXT,
    created_by                TEXT,
    version                   BIGINT NOT NULL DEFAULT 1,
    -- M17_001 §2.1: monthly token budget (tokens per calendar month)
    -- Canonical constant: src/types/defaults.zig#DEFAULT_WORKSPACE_MONTHLY_TOKEN_BUDGET
    monthly_token_budget      BIGINT NOT NULL DEFAULT 10000000,
    created_at                BIGINT NOT NULL,
    updated_at                BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workspaces_tenant ON core.workspaces(tenant_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_workspaces_tenant_name
    ON core.workspaces(tenant_id, name) WHERE name IS NOT NULL;
