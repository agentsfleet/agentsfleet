-- UseZombie clean-state role and schema privilege baseline

CREATE SCHEMA IF NOT EXISTS vault;
CREATE SCHEMA IF NOT EXISTS ops_ro;
CREATE SCHEMA IF NOT EXISTS memory;

DO $$
DECLARE
    r text;
BEGIN
    FOREACH r IN ARRAY ARRAY[
        'db_migrator',
        'api_runtime',
        'worker_runtime',
        'memory_runtime',
        'ops_readonly_human',
        'ops_readonly_agent'
    ]
    LOOP
        IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = r) THEN
            EXECUTE format('CREATE ROLE %I NOLOGIN', r);
        END IF;
    END LOOP;
END
$$;

REVOKE CREATE ON SCHEMA public FROM PUBLIC;

-- db_migrator: full DDL authority (control plane only)
GRANT ALL ON SCHEMA public, core, agent, billing, vault, audit, ops_ro, memory TO db_migrator;
GRANT ALL ON ALL TABLES IN SCHEMA core, agent, billing, vault, audit, ops_ro, memory TO db_migrator;

-- Runtime roles: data access only
GRANT USAGE ON SCHEMA core, agent, billing, vault, audit TO api_runtime, worker_runtime;

-- M10_001: Pipeline v1 removed. Grants to dropped tables removed:
-- core.specs, core.runs, core.run_transitions, core.artifacts,
-- core.workspace_memories, core.policy_events, billing.usage_ledger.
GRANT SELECT, INSERT, UPDATE, DELETE ON
    core.tenants,
    core.workspaces
TO api_runtime;

GRANT SELECT ON
    core.tenants,
    core.workspaces
TO worker_runtime;

-- audit schema: runtime read-only migration state inspection
GRANT SELECT ON audit.schema_migrations, audit.schema_migration_failures TO api_runtime, worker_runtime;

CREATE TABLE IF NOT EXISTS vault.secrets (
    id            UUID PRIMARY KEY,
    CONSTRAINT ck_vault_secrets_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id  UUID    NOT NULL REFERENCES core.workspaces(workspace_id),
    key_name      TEXT    NOT NULL,
    kek_version   INTEGER NOT NULL DEFAULT 1,
    encrypted_dek BYTEA   NOT NULL,
    dek_nonce     BYTEA   NOT NULL,
    dek_tag       BYTEA   NOT NULL,
    nonce         BYTEA   NOT NULL,
    ciphertext    BYTEA   NOT NULL,
    tag           BYTEA   NOT NULL,
    created_at    BIGINT  NOT NULL,
    updated_at    BIGINT  NOT NULL,
    UNIQUE (workspace_id, key_name)
);

CREATE INDEX IF NOT EXISTS idx_vault_secrets_workspace
    ON vault.secrets(workspace_id, key_name);

GRANT SELECT, INSERT, UPDATE ON vault.secrets TO api_runtime, worker_runtime;

-- Workspace-scoped skill secrets (BYOK credentials per skill+key).
CREATE TABLE IF NOT EXISTS vault.workspace_skill_secrets (
    id               UUID PRIMARY KEY,
    CONSTRAINT ck_workspace_skill_secrets_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id        UUID NOT NULL REFERENCES core.tenants(tenant_id),
    workspace_id     UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    skill_ref        TEXT NOT NULL,
    key_name         TEXT NOT NULL,
    scope            TEXT NOT NULL,
    secret_meta_json TEXT NOT NULL DEFAULT '{}',
    kek_version      INTEGER NOT NULL DEFAULT 1,
    encrypted_dek    BYTEA NOT NULL,
    dek_nonce        BYTEA NOT NULL,
    dek_tag          BYTEA NOT NULL,
    nonce            BYTEA NOT NULL,
    ciphertext       BYTEA NOT NULL,
    tag              BYTEA NOT NULL,
    created_at       BIGINT NOT NULL,
    updated_at       BIGINT NOT NULL,
    UNIQUE (workspace_id, skill_ref, key_name)
);
CREATE INDEX IF NOT EXISTS idx_workspace_skill_secrets_lookup
    ON vault.workspace_skill_secrets(workspace_id, skill_ref, key_name);
CREATE INDEX IF NOT EXISTS idx_workspace_skill_secrets_tenant
    ON vault.workspace_skill_secrets(tenant_id, workspace_id, created_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON vault.workspace_skill_secrets TO api_runtime;
GRANT SELECT ON vault.workspace_skill_secrets TO worker_runtime;

-- Read-only principals are strictly routed to ops_ro + audit
GRANT USAGE ON SCHEMA ops_ro, audit TO ops_readonly_human, ops_readonly_agent;

-- No runtime/read-only DDL in app schemas
REVOKE CREATE ON SCHEMA public, core, agent, billing, vault, audit, ops_ro, memory
FROM api_runtime, worker_runtime, memory_runtime, ops_readonly_human, ops_readonly_agent;

-- Remove default PUBLIC table visibility in authoritative app schemas.
REVOKE ALL ON ALL TABLES IN SCHEMA core, agent, billing, vault, audit, ops_ro, memory FROM PUBLIC;

ALTER ROLE api_runtime SET search_path = core, agent, billing, vault, audit, public;
ALTER ROLE worker_runtime SET search_path = core, agent, billing, vault, audit, public;
ALTER ROLE ops_readonly_human SET search_path = ops_ro, audit, public;
ALTER ROLE ops_readonly_agent SET search_path = ops_ro, audit, public;

-- Keep local-superuser test connections deterministic when role-specific
-- URLs are not used (e.g. HANDLER_DB_TEST_URL in CI/dev Docker).
DO $$
BEGIN
    EXECUTE format(
        'ALTER DATABASE %I SET search_path = core,agent,billing,vault,audit,public',
        current_database()
    );
END
$$;
