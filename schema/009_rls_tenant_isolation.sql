-- Tenant isolation hardening + prompt lifecycle observability (clean-state)

CREATE TABLE IF NOT EXISTS core.prompt_lifecycle_events (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_prompt_lifecycle_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    event_id            TEXT NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    workspace_id        UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    tenant_id           UUID NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    agent_id            UUID,
    config_version_id   UUID,
    metadata_json       TEXT NOT NULL DEFAULT '{}',
    created_at          BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_prompt_lifecycle_events_workspace
    ON core.prompt_lifecycle_events(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_prompt_lifecycle_events_tenant
    ON core.prompt_lifecycle_events(tenant_id, created_at DESC);

CREATE OR REPLACE FUNCTION reject_prompt_lifecycle_event_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'core.prompt_lifecycle_events is append-only';
END;
$$;

DO $$ BEGIN
    CREATE TRIGGER trg_prompt_lifecycle_events_no_update
        BEFORE UPDATE ON core.prompt_lifecycle_events
        FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation();
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
    CREATE TRIGGER trg_prompt_lifecycle_events_no_delete
        BEFORE DELETE ON core.prompt_lifecycle_events
        FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation();
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

ALTER TABLE agent.agent_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent.agent_config_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent.workspace_active_config ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent.config_compile_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE vault.workspace_skill_secrets ENABLE ROW LEVEL SECURITY;

-- NOTE: EXCEPTION WHEN duplicate_object silently skips if the policy already exists.
-- To change a policy definition, write a new migration that DROPs the old policy first.
DO $$ BEGIN CREATE POLICY agent_profiles_select_tenant ON agent.agent_profiles FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY agent_profiles_insert_tenant ON agent.agent_profiles FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY agent_profiles_update_tenant ON agent.agent_profiles FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true)) WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY agent_profiles_delete_tenant ON agent.agent_profiles FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN CREATE POLICY agent_config_versions_select_tenant ON agent.agent_config_versions FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY agent_config_versions_insert_tenant ON agent.agent_config_versions FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY agent_config_versions_update_tenant ON agent.agent_config_versions FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true)) WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY agent_config_versions_delete_tenant ON agent.agent_config_versions FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN CREATE POLICY workspace_active_config_select_tenant ON agent.workspace_active_config FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY workspace_active_config_insert_tenant ON agent.workspace_active_config FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY workspace_active_config_update_tenant ON agent.workspace_active_config FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true)) WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY workspace_active_config_delete_tenant ON agent.workspace_active_config FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN CREATE POLICY config_compile_jobs_select_tenant ON agent.config_compile_jobs FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY config_compile_jobs_insert_tenant ON agent.config_compile_jobs FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY config_compile_jobs_update_tenant ON agent.config_compile_jobs FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true)) WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY config_compile_jobs_delete_tenant ON agent.config_compile_jobs FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN CREATE POLICY workspace_skill_secrets_select_tenant ON vault.workspace_skill_secrets FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY workspace_skill_secrets_insert_tenant ON vault.workspace_skill_secrets FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY workspace_skill_secrets_update_tenant ON vault.workspace_skill_secrets FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true)) WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
DO $$ BEGIN CREATE POLICY workspace_skill_secrets_delete_tenant ON vault.workspace_skill_secrets FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true)); EXCEPTION WHEN duplicate_object THEN NULL; END $$;

GRANT SELECT, INSERT ON core.prompt_lifecycle_events TO api_runtime, worker_runtime;
