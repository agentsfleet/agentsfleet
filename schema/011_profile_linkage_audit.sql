-- Immutable compile/activate/run linkage artifacts (UUID-only IDs)

CREATE TABLE IF NOT EXISTS agent.config_linkage_audit_artifacts (
    artifact_id          UUID PRIMARY KEY,
    tenant_id            UUID NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    workspace_id         UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    artifact_type        TEXT NOT NULL,
    config_version_id    UUID NOT NULL REFERENCES agent.agent_config_versions(config_version_id) ON DELETE RESTRICT,
    compile_job_id       UUID REFERENCES agent.config_compile_jobs(compile_job_id) ON DELETE RESTRICT,
    run_id               UUID REFERENCES core.runs(run_id) ON DELETE CASCADE,
    parent_artifact_id   UUID REFERENCES agent.config_linkage_audit_artifacts(artifact_id) ON DELETE RESTRICT,
    metadata_json        TEXT NOT NULL DEFAULT '{}',
    created_at           BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_config_linkage_workspace
    ON agent.config_linkage_audit_artifacts(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_config_linkage_config_version
    ON agent.config_linkage_audit_artifacts(config_version_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS uq_config_linkage_run_artifact
    ON agent.config_linkage_audit_artifacts(run_id)
    WHERE run_id IS NOT NULL;

CREATE OR REPLACE FUNCTION reject_config_linkage_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'agent.config_linkage_audit_artifacts is append-only';
END;
$$;

CREATE TRIGGER trg_config_linkage_no_update
    BEFORE UPDATE ON agent.config_linkage_audit_artifacts
    FOR EACH ROW EXECUTE FUNCTION reject_config_linkage_mutation();

CREATE TRIGGER trg_config_linkage_no_delete
    BEFORE DELETE ON agent.config_linkage_audit_artifacts
    FOR EACH ROW EXECUTE FUNCTION reject_config_linkage_mutation();

ALTER TABLE agent.config_linkage_audit_artifacts ENABLE ROW LEVEL SECURITY;

CREATE POLICY config_linkage_select_tenant ON agent.config_linkage_audit_artifacts
    FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY config_linkage_insert_tenant ON agent.config_linkage_audit_artifacts
    FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));

GRANT SELECT, INSERT ON agent.config_linkage_audit_artifacts TO api_runtime;
GRANT SELECT ON agent.config_linkage_audit_artifacts TO worker_runtime;
