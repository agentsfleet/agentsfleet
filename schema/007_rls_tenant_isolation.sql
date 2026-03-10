-- UseZombie M5_002 tenant isolation hardening + prompt lifecycle observability

ALTER TABLE agent_profile_versions ADD COLUMN IF NOT EXISTS tenant_id TEXT;
UPDATE agent_profile_versions v
SET tenant_id = p.tenant_id
FROM agent_profiles p
WHERE v.profile_id = p.profile_id
  AND v.tenant_id IS NULL;
ALTER TABLE agent_profile_versions ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE agent_profile_versions
    ADD CONSTRAINT fk_agent_profile_versions_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id);
CREATE INDEX IF NOT EXISTS idx_agent_profile_versions_tenant ON agent_profile_versions(tenant_id, created_at DESC);

ALTER TABLE workspace_active_profile ADD COLUMN IF NOT EXISTS tenant_id TEXT;
UPDATE workspace_active_profile wap
SET tenant_id = w.tenant_id
FROM workspaces w
WHERE wap.workspace_id = w.workspace_id
  AND wap.tenant_id IS NULL;
ALTER TABLE workspace_active_profile ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE workspace_active_profile
    ADD CONSTRAINT fk_workspace_active_profile_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id);
CREATE INDEX IF NOT EXISTS idx_workspace_active_profile_tenant ON workspace_active_profile(tenant_id, activated_at DESC);

ALTER TABLE profile_compile_jobs ADD COLUMN IF NOT EXISTS tenant_id TEXT;
UPDATE profile_compile_jobs j
SET tenant_id = w.tenant_id
FROM workspaces w
WHERE j.workspace_id = w.workspace_id
  AND j.tenant_id IS NULL;
ALTER TABLE profile_compile_jobs ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE profile_compile_jobs
    ADD CONSTRAINT fk_profile_compile_jobs_tenant
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id);
CREATE INDEX IF NOT EXISTS idx_profile_compile_jobs_tenant ON profile_compile_jobs(tenant_id, created_at DESC);

ALTER TABLE vault.workspace_skill_secrets ADD COLUMN IF NOT EXISTS tenant_id TEXT;
UPDATE vault.workspace_skill_secrets s
SET tenant_id = w.tenant_id
FROM public.workspaces w
WHERE s.workspace_id = w.workspace_id
  AND s.tenant_id IS NULL;
ALTER TABLE vault.workspace_skill_secrets ALTER COLUMN tenant_id SET NOT NULL;
ALTER TABLE vault.workspace_skill_secrets
    ADD CONSTRAINT fk_workspace_skill_secrets_tenant
    FOREIGN KEY (tenant_id) REFERENCES public.tenants(tenant_id);
CREATE INDEX IF NOT EXISTS idx_workspace_skill_secrets_tenant
    ON vault.workspace_skill_secrets(tenant_id, workspace_id, created_at DESC);

CREATE TABLE IF NOT EXISTS prompt_lifecycle_events (
    id                  BIGSERIAL PRIMARY KEY,
    event_id            TEXT NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    workspace_id        TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    tenant_id           TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    profile_id          TEXT,
    profile_version_id  TEXT,
    metadata_json       TEXT NOT NULL DEFAULT '{}',
    created_at          BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_prompt_lifecycle_events_workspace
    ON prompt_lifecycle_events(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_prompt_lifecycle_events_tenant
    ON prompt_lifecycle_events(tenant_id, created_at DESC);

CREATE OR REPLACE FUNCTION reject_prompt_lifecycle_event_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'prompt_lifecycle_events is append-only';
END;
$$;

DROP TRIGGER IF EXISTS trg_prompt_lifecycle_events_no_update ON prompt_lifecycle_events;
CREATE TRIGGER trg_prompt_lifecycle_events_no_update
    BEFORE UPDATE ON prompt_lifecycle_events
    FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation();

DROP TRIGGER IF EXISTS trg_prompt_lifecycle_events_no_delete ON prompt_lifecycle_events;
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

DROP POLICY IF EXISTS agent_profiles_select_tenant ON agent_profiles;
DROP POLICY IF EXISTS agent_profiles_insert_tenant ON agent_profiles;
DROP POLICY IF EXISTS agent_profiles_update_tenant ON agent_profiles;
DROP POLICY IF EXISTS agent_profiles_delete_tenant ON agent_profiles;
CREATE POLICY agent_profiles_select_tenant ON agent_profiles
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profiles_insert_tenant ON agent_profiles
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profiles_update_tenant ON agent_profiles
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profiles_delete_tenant ON agent_profiles
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true));

DROP POLICY IF EXISTS agent_profile_versions_select_tenant ON agent_profile_versions;
DROP POLICY IF EXISTS agent_profile_versions_insert_tenant ON agent_profile_versions;
DROP POLICY IF EXISTS agent_profile_versions_update_tenant ON agent_profile_versions;
DROP POLICY IF EXISTS agent_profile_versions_delete_tenant ON agent_profile_versions;
CREATE POLICY agent_profile_versions_select_tenant ON agent_profile_versions
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profile_versions_insert_tenant ON agent_profile_versions
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profile_versions_update_tenant ON agent_profile_versions
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY agent_profile_versions_delete_tenant ON agent_profile_versions
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true));

DROP POLICY IF EXISTS workspace_active_profile_select_tenant ON workspace_active_profile;
DROP POLICY IF EXISTS workspace_active_profile_insert_tenant ON workspace_active_profile;
DROP POLICY IF EXISTS workspace_active_profile_update_tenant ON workspace_active_profile;
DROP POLICY IF EXISTS workspace_active_profile_delete_tenant ON workspace_active_profile;
CREATE POLICY workspace_active_profile_select_tenant ON workspace_active_profile
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_active_profile_insert_tenant ON workspace_active_profile
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_active_profile_update_tenant ON workspace_active_profile
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_active_profile_delete_tenant ON workspace_active_profile
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true));

DROP POLICY IF EXISTS profile_compile_jobs_select_tenant ON profile_compile_jobs;
DROP POLICY IF EXISTS profile_compile_jobs_insert_tenant ON profile_compile_jobs;
DROP POLICY IF EXISTS profile_compile_jobs_update_tenant ON profile_compile_jobs;
DROP POLICY IF EXISTS profile_compile_jobs_delete_tenant ON profile_compile_jobs;
CREATE POLICY profile_compile_jobs_select_tenant ON profile_compile_jobs
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY profile_compile_jobs_insert_tenant ON profile_compile_jobs
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY profile_compile_jobs_update_tenant ON profile_compile_jobs
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY profile_compile_jobs_delete_tenant ON profile_compile_jobs
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true));

DROP POLICY IF EXISTS workspace_skill_secrets_select_tenant ON vault.workspace_skill_secrets;
DROP POLICY IF EXISTS workspace_skill_secrets_insert_tenant ON vault.workspace_skill_secrets;
DROP POLICY IF EXISTS workspace_skill_secrets_update_tenant ON vault.workspace_skill_secrets;
DROP POLICY IF EXISTS workspace_skill_secrets_delete_tenant ON vault.workspace_skill_secrets;
CREATE POLICY workspace_skill_secrets_select_tenant ON vault.workspace_skill_secrets
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_skill_secrets_insert_tenant ON vault.workspace_skill_secrets
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_skill_secrets_update_tenant ON vault.workspace_skill_secrets
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true))
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY workspace_skill_secrets_delete_tenant ON vault.workspace_skill_secrets
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true));

GRANT SELECT, INSERT ON prompt_lifecycle_events TO api_accessor, worker_accessor;
GRANT USAGE, SELECT ON SEQUENCE prompt_lifecycle_events_id_seq TO api_accessor, worker_accessor;

DO $$
DECLARE
    owner_name TEXT;
BEGIN
    FOR owner_name IN
        SELECT pg_get_userbyid(c.relowner)
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE (n.nspname = 'public' AND c.relname IN (
            'agent_profiles',
            'agent_profile_versions',
            'workspace_active_profile',
            'profile_compile_jobs',
            'prompt_lifecycle_events'
        ))
           OR (n.nspname = 'vault' AND c.relname IN ('workspace_skill_secrets'))
    LOOP
        IF owner_name IN ('api_accessor', 'worker_accessor') THEN
            RAISE EXCEPTION 'RLS bypass risk: table owner must not be api_accessor/worker_accessor';
        END IF;
    END LOOP;
END;
$$;
