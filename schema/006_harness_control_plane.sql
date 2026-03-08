-- UseZombie M5_002 harness control plane schema

CREATE TABLE IF NOT EXISTS agent_profiles (
    profile_id    TEXT PRIMARY KEY,
    tenant_id     TEXT NOT NULL REFERENCES tenants(tenant_id),
    workspace_id  TEXT NOT NULL REFERENCES workspaces(workspace_id),
    name          TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'DRAFT' CHECK (status IN ('DRAFT', 'ACTIVE', 'ARCHIVED')),
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_agent_profiles_workspace ON agent_profiles(workspace_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS agent_profile_versions (
    profile_version_id      TEXT PRIMARY KEY,
    profile_id              TEXT NOT NULL REFERENCES agent_profiles(profile_id) ON DELETE CASCADE,
    version                 INTEGER NOT NULL,
    source_markdown         TEXT NOT NULL,
    compiled_profile_json   TEXT,
    compile_engine          TEXT NOT NULL DEFAULT 'deterministic-v1',
    validation_report_json  TEXT NOT NULL DEFAULT '{}',
    is_valid                BOOLEAN NOT NULL DEFAULT FALSE,
    created_at              BIGINT NOT NULL,
    updated_at              BIGINT NOT NULL,
    UNIQUE (profile_id, version)
);
CREATE INDEX IF NOT EXISTS idx_agent_profile_versions_profile ON agent_profile_versions(profile_id, version DESC);

CREATE TABLE IF NOT EXISTS workspace_active_profile (
    workspace_id        TEXT PRIMARY KEY REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    profile_version_id  TEXT NOT NULL REFERENCES agent_profile_versions(profile_version_id),
    activated_by        TEXT NOT NULL,
    activated_at        BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS profile_compile_jobs (
    compile_job_id        TEXT PRIMARY KEY,
    workspace_id          TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    requested_profile_id  TEXT NOT NULL REFERENCES agent_profiles(profile_id) ON DELETE CASCADE,
    requested_version     INTEGER NOT NULL,
    state                 TEXT NOT NULL CHECK (state IN ('QUEUED', 'RUNNING', 'SUCCEEDED', 'FAILED')),
    failure_reason        TEXT,
    validation_report_json TEXT NOT NULL DEFAULT '{}',
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_profile_compile_jobs_workspace ON profile_compile_jobs(workspace_id, created_at DESC);

CREATE TABLE IF NOT EXISTS vault.workspace_skill_secrets (
    id            BIGSERIAL PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES public.workspaces(workspace_id) ON DELETE CASCADE,
    skill_ref     TEXT NOT NULL,
    key_name      TEXT NOT NULL,
    scope         TEXT NOT NULL DEFAULT 'sandbox' CHECK (scope IN ('host', 'sandbox')),
    secret_meta_json TEXT NOT NULL DEFAULT '{}',
    kek_version   INTEGER NOT NULL DEFAULT 1,
    encrypted_dek BYTEA NOT NULL,
    dek_nonce     BYTEA NOT NULL,
    dek_tag       BYTEA NOT NULL,
    nonce         BYTEA NOT NULL,
    ciphertext    BYTEA NOT NULL,
    tag           BYTEA NOT NULL,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    UNIQUE (workspace_id, skill_ref, key_name)
);
CREATE INDEX IF NOT EXISTS idx_workspace_skill_secrets_lookup
    ON vault.workspace_skill_secrets(workspace_id, skill_ref, key_name);

GRANT SELECT, INSERT, UPDATE, DELETE ON
    agent_profiles, agent_profile_versions, workspace_active_profile, profile_compile_jobs
TO api_accessor;

GRANT SELECT ON
    agent_profiles, agent_profile_versions, workspace_active_profile
TO worker_accessor;

GRANT SELECT, INSERT, UPDATE, DELETE ON vault.workspace_skill_secrets TO api_accessor;
GRANT SELECT ON vault.workspace_skill_secrets TO worker_accessor;
GRANT USAGE, SELECT ON SEQUENCE vault.workspace_skill_secrets_id_seq TO api_accessor, worker_accessor;
