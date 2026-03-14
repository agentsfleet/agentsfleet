-- Tenant isolation hardening + prompt lifecycle observability (clean-state)

CREATE TABLE prompt_lifecycle_events (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_prompt_lifecycle_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    event_id            TEXT NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    workspace_id        UUID NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    tenant_id           UUID NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    profile_id          UUID,
    profile_version_id  UUID,
    metadata_json       TEXT NOT NULL DEFAULT '{}',
    created_at          BIGINT NOT NULL
);
CREATE INDEX idx_prompt_lifecycle_events_workspace
    ON prompt_lifecycle_events(workspace_id, created_at DESC);
CREATE INDEX idx_prompt_lifecycle_events_tenant
    ON prompt_lifecycle_events(tenant_id, created_at DESC);

CREATE OR REPLACE FUNCTION reject_prompt_lifecycle_event_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'prompt_lifecycle_events is append-only';
END;
$$;

CREATE TRIGGER trg_prompt_lifecycle_events_no_update
    BEFORE UPDATE ON prompt_lifecycle_events
    FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation();

CREATE TRIGGER trg_prompt_lifecycle_events_no_delete
    BEFORE DELETE ON prompt_lifecycle_events
    FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation();

ALTER TABLE agent_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent_profile_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_active_profile ENABLE ROW LEVEL SECURITY;
ALTER TABLE profile_compile_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE vault.workspace_skill_secrets ENABLE ROW LEVEL SECURITY;

ALTER TABLE agent_profiles FORCE ROW LEVEL SECURITY;
ALTER TABLE agent_profile_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE workspace_active_profile FORCE ROW LEVEL SECURITY;
ALTER TABLE profile_compile_jobs FORCE ROW LEVEL SECURITY;
ALTER TABLE vault.workspace_skill_secrets FORCE ROW LEVEL SECURITY;

CREATE POLICY agent_profiles_select_tenant ON agent_profiles
    FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profiles_insert_tenant ON agent_profiles
    FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profiles_update_tenant ON agent_profiles
    FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profiles_delete_tenant ON agent_profiles
    FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true));

CREATE POLICY agent_profile_versions_select_tenant ON agent_profile_versions
    FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profile_versions_insert_tenant ON agent_profile_versions
    FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profile_versions_update_tenant ON agent_profile_versions
    FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profile_versions_delete_tenant ON agent_profile_versions
    FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true));

CREATE POLICY workspace_active_profile_select_tenant ON workspace_active_profile
    FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_active_profile_insert_tenant ON workspace_active_profile
    FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_active_profile_update_tenant ON workspace_active_profile
    FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_active_profile_delete_tenant ON workspace_active_profile
    FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true));

CREATE POLICY profile_compile_jobs_select_tenant ON profile_compile_jobs
    FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY profile_compile_jobs_insert_tenant ON profile_compile_jobs
    FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY profile_compile_jobs_update_tenant ON profile_compile_jobs
    FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY profile_compile_jobs_delete_tenant ON profile_compile_jobs
    FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true));

CREATE POLICY workspace_skill_secrets_select_tenant ON vault.workspace_skill_secrets
    FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_skill_secrets_insert_tenant ON vault.workspace_skill_secrets
    FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_skill_secrets_update_tenant ON vault.workspace_skill_secrets
    FOR UPDATE USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_skill_secrets_delete_tenant ON vault.workspace_skill_secrets
    FOR DELETE USING (tenant_id::text = current_setting('app.current_tenant_id', true));

GRANT SELECT, INSERT ON prompt_lifecycle_events TO api_accessor, worker_accessor;

