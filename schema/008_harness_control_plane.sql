-- UseZombie harness control plane schema (UUID-only IDs)

CREATE TABLE IF NOT EXISTS agent.agent_profiles (
    agent_id      UUID PRIMARY KEY,
    tenant_id     UUID NOT NULL REFERENCES core.tenants(tenant_id),
    workspace_id  UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    name          TEXT NOT NULL,
    status        TEXT NOT NULL,
    trust_streak_runs INTEGER NOT NULL,
    trust_level   TEXT NOT NULL,
    last_scored_at BIGINT,
    created_at    BIGINT NOT NULL,
    updated_at    BIGINT NOT NULL,
    CONSTRAINT ck_agent_profiles_trust_level CHECK (trust_level IN ('UNEARNED', 'TRUSTED'))
);
CREATE INDEX IF NOT EXISTS idx_agent_profiles_workspace_agent ON agent.agent_profiles(workspace_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS agent.agent_config_versions (
    config_version_id       UUID PRIMARY KEY,
    tenant_id               UUID NOT NULL REFERENCES core.tenants(tenant_id),
    agent_id                UUID NOT NULL REFERENCES agent.agent_profiles(agent_id) ON DELETE CASCADE,
    version                 INTEGER NOT NULL,
    source_markdown         TEXT NOT NULL,
    compiled_profile_json   TEXT,
    compile_engine          TEXT NOT NULL,
    validation_report_json  TEXT NOT NULL DEFAULT '{}',
    is_valid                BOOLEAN NOT NULL DEFAULT FALSE,
    created_at              BIGINT NOT NULL,
    updated_at              BIGINT NOT NULL,
    UNIQUE (agent_id, version),
    CONSTRAINT ck_agent_config_versions_uuidv7 CHECK (substring(config_version_id::text from 15 for 1) = '7')
);
CREATE INDEX IF NOT EXISTS idx_agent_config_versions_agent ON agent.agent_config_versions(agent_id, version DESC);
CREATE INDEX IF NOT EXISTS idx_agent_config_versions_tenant ON agent.agent_config_versions(tenant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS agent.workspace_active_config (
    workspace_id       UUID PRIMARY KEY REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    tenant_id          UUID NOT NULL REFERENCES core.tenants(tenant_id),
    config_version_id  UUID NOT NULL REFERENCES agent.agent_config_versions(config_version_id),
    activated_by       TEXT NOT NULL,
    activated_at       BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workspace_active_config_tenant ON agent.workspace_active_config(tenant_id, activated_at DESC);

CREATE TABLE IF NOT EXISTS agent.agent_improvement_proposals (
    proposal_id         UUID PRIMARY KEY,
    agent_id            UUID NOT NULL REFERENCES agent.agent_profiles(agent_id) ON DELETE CASCADE,
    workspace_id        UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    trigger_reason      TEXT NOT NULL,
    proposed_changes    TEXT NOT NULL,
    config_version_id   UUID NOT NULL REFERENCES agent.agent_config_versions(config_version_id),
    approval_mode       TEXT NOT NULL,
    generation_status   TEXT NOT NULL,
    status              TEXT NOT NULL,
    rejection_reason    TEXT,
    auto_apply_at       BIGINT,
    applied_by          TEXT,
    created_at          BIGINT NOT NULL,
    updated_at          BIGINT NOT NULL,
    CONSTRAINT ck_agent_improvement_proposals_uuidv7 CHECK (substring(proposal_id::text from 15 for 1) = '7')
);
CREATE INDEX IF NOT EXISTS idx_agent_improvement_proposals_agent
    ON agent.agent_improvement_proposals(agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_improvement_proposals_veto_window
    ON agent.agent_improvement_proposals(status, auto_apply_at);

CREATE TABLE IF NOT EXISTS agent.harness_change_log (
    change_id      UUID PRIMARY KEY,
    agent_id       UUID NOT NULL REFERENCES agent.agent_profiles(agent_id) ON DELETE CASCADE,
    proposal_id    UUID NOT NULL REFERENCES agent.agent_improvement_proposals(proposal_id),
    workspace_id   UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    field_name     TEXT NOT NULL,
    old_value      TEXT NOT NULL,
    new_value      TEXT NOT NULL,
    applied_at     BIGINT NOT NULL,
    applied_by     TEXT NOT NULL,
    reverted_from  UUID REFERENCES agent.harness_change_log(change_id),
    score_delta    DOUBLE PRECISION,
    CONSTRAINT ck_harness_change_log_uuidv7 CHECK (substring(change_id::text from 15 for 1) = '7')
);
CREATE INDEX IF NOT EXISTS idx_harness_change_log_agent ON agent.harness_change_log(agent_id, applied_at DESC);

CREATE TABLE IF NOT EXISTS agent.config_compile_jobs (
    compile_job_id        UUID PRIMARY KEY,
    tenant_id             UUID NOT NULL REFERENCES core.tenants(tenant_id),
    workspace_id          UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    requested_agent_id    UUID NOT NULL REFERENCES agent.agent_profiles(agent_id) ON DELETE CASCADE,
    requested_version     INTEGER NOT NULL,
    state                 TEXT NOT NULL,
    failure_reason        TEXT,
    validation_report_json TEXT NOT NULL DEFAULT '{}',
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL,
    CONSTRAINT ck_config_compile_jobs_uuidv7 CHECK (substring(compile_job_id::text from 15 for 1) = '7')
);
CREATE INDEX IF NOT EXISTS idx_config_compile_jobs_workspace ON agent.config_compile_jobs(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_config_compile_jobs_tenant ON agent.config_compile_jobs(tenant_id, created_at DESC);

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

GRANT SELECT, INSERT, UPDATE, DELETE ON
    agent.agent_profiles, agent.agent_config_versions, agent.workspace_active_config, agent.config_compile_jobs
TO api_runtime;

GRANT SELECT, UPDATE ON
    agent.agent_improvement_proposals
TO api_runtime;

GRANT SELECT, INSERT ON
    agent.harness_change_log
TO api_runtime;

GRANT SELECT, UPDATE ON
    agent.agent_profiles, agent.workspace_active_config
TO worker_runtime;

GRANT SELECT, INSERT ON
    agent.agent_config_versions
TO worker_runtime;

GRANT SELECT, INSERT, UPDATE ON
    agent.agent_improvement_proposals
TO worker_runtime;

GRANT SELECT, INSERT, UPDATE ON
    agent.harness_change_log
TO worker_runtime;

GRANT SELECT, INSERT, UPDATE, DELETE ON vault.workspace_skill_secrets TO api_runtime;
GRANT SELECT ON vault.workspace_skill_secrets TO worker_runtime;
