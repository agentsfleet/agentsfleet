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

-- Every `ck_*_uid_uuidv7` constraint below, and the ones repeating this pattern
-- in the sibling schema files, deliberately pins the VERSION nibble ONLY.
--
-- It is a smoke alarm, not the authority. Every `uid` is server-minted by
-- `id_format.generateUuidV7` (src/agentsfleetd/types/id_format.zig), which is
-- the sole writer: there is no DEFAULT, no external loader, and no request
-- field is ever bound to a `uid` column. The variant bits and the lowercase
-- canonical form are pinned there by test, so the only way a malformed uid
-- could appear is a regression in that generator -- which its unit tests catch
-- at build time, with a better message than a constraint violation.
--
-- A full-shape regex here was measured at 17x this check's cost per row inline,
-- and 56x via a shared IMMUTABLE function (a SET search_path clause blocks
-- Postgres from inlining it). On a per-event table that is a permanent tax to
-- re-catch what the tests already catch, so the cheap check stays.
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
