-- UseZombie harness control plane schema (UUID-only IDs)

CREATE TABLE agent_profiles (
    profile_id    UUID PRIMARY KEY,
    tenant_id     UUID NOT NULL REFERENCES tenants(tenant_id),
    workspace_id  UUID NOT NULL REFERENCES workspaces(workspace_id),
    name          TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'DRAFT' CHECK (status IN ('DRAFT', 'ACTIVE', 'ARCHIVED')),
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL
);
CREATE INDEX idx_agent_profiles_workspace ON agent_profiles(workspace_id, updated_at DESC);

CREATE TABLE agent_profile_versions (
    profile_version_id      UUID PRIMARY KEY,
    tenant_id               UUID NOT NULL REFERENCES tenants(tenant_id),
    profile_id              UUID NOT NULL REFERENCES agent_profiles(profile_id) ON DELETE CASCADE,
    version                 INTEGER NOT NULL,
    source_markdown         TEXT NOT NULL,
    compiled_profile_json   TEXT,
    compile_engine          TEXT NOT NULL DEFAULT 'deterministic-v1',
    validation_report_json  TEXT NOT NULL DEFAULT '{}',
    is_valid                BOOLEAN NOT NULL DEFAULT FALSE,
    created_at              BIGINT NOT NULL,
    updated_at              BIGINT NOT NULL,
    UNIQUE (profile_id, version),
    CONSTRAINT ck_agent_profile_versions_uuidv7 CHECK (substring(profile_version_id::text from 15 for 1) = '7')
);
CREATE INDEX idx_agent_profile_versions_profile ON agent_profile_versions(profile_id, version DESC);
CREATE INDEX idx_agent_profile_versions_tenant ON agent_profile_versions(tenant_id, created_at DESC);

CREATE TABLE workspace_active_profile (
    workspace_id        UUID PRIMARY KEY REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    tenant_id           UUID NOT NULL REFERENCES tenants(tenant_id),
    profile_version_id  UUID NOT NULL REFERENCES agent_profile_versions(profile_version_id),
    activated_by        TEXT NOT NULL,
    activated_at        BIGINT NOT NULL
);
CREATE INDEX idx_workspace_active_profile_tenant ON workspace_active_profile(tenant_id, activated_at DESC);

CREATE TABLE profile_compile_jobs (
    compile_job_id         UUID PRIMARY KEY,
    tenant_id              UUID NOT NULL REFERENCES tenants(tenant_id),
    workspace_id           UUID NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    requested_profile_id   UUID NOT NULL REFERENCES agent_profiles(profile_id) ON DELETE CASCADE,
    requested_version      INTEGER NOT NULL,
    state                  TEXT NOT NULL CHECK (state IN ('QUEUED', 'RUNNING', 'SUCCEEDED', 'FAILED')),
    failure_reason         TEXT,
    validation_report_json TEXT NOT NULL DEFAULT '{}',
    created_at             BIGINT NOT NULL,
    updated_at             BIGINT NOT NULL,
    CONSTRAINT ck_profile_compile_jobs_uuidv7 CHECK (substring(compile_job_id::text from 15 for 1) = '7')
);
CREATE INDEX idx_profile_compile_jobs_workspace ON profile_compile_jobs(workspace_id, created_at DESC);
CREATE INDEX idx_profile_compile_jobs_tenant ON profile_compile_jobs(tenant_id, created_at DESC);

CREATE TABLE vault.workspace_skill_secrets (
    id               BIGSERIAL PRIMARY KEY,
    tenant_id        UUID NOT NULL REFERENCES public.tenants(tenant_id),
    workspace_id     UUID NOT NULL REFERENCES public.workspaces(workspace_id) ON DELETE CASCADE,
    skill_ref        TEXT NOT NULL,
    key_name         TEXT NOT NULL,
    scope            TEXT NOT NULL DEFAULT 'sandbox' CHECK (scope IN ('host', 'sandbox')),
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
CREATE INDEX idx_workspace_skill_secrets_lookup
    ON vault.workspace_skill_secrets(workspace_id, skill_ref, key_name);
CREATE INDEX idx_workspace_skill_secrets_tenant
    ON vault.workspace_skill_secrets(tenant_id, workspace_id, created_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON
    agent_profiles, agent_profile_versions, workspace_active_profile, profile_compile_jobs
TO api_accessor;

GRANT SELECT ON
    agent_profiles, agent_profile_versions, workspace_active_profile
TO worker_accessor;

GRANT SELECT, INSERT, UPDATE, DELETE ON vault.workspace_skill_secrets TO api_accessor;
GRANT SELECT ON vault.workspace_skill_secrets TO worker_accessor;
GRANT USAGE, SELECT ON SEQUENCE vault.workspace_skill_secrets_id_seq TO api_accessor, worker_accessor;
