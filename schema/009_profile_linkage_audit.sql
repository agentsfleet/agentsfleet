-- Immutable compile/activate/run linkage artifacts (UUID-only IDs)

CREATE TABLE config_linkage_audit_artifacts (
    artifact_id          UUID PRIMARY KEY,
    tenant_id            UUID NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    workspace_id         UUID NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    artifact_type        TEXT NOT NULL,
    config_version_id    UUID NOT NULL REFERENCES agent_config_versions(config_version_id) ON DELETE RESTRICT,
    compile_job_id       UUID REFERENCES config_compile_jobs(compile_job_id) ON DELETE RESTRICT,
    run_id               UUID REFERENCES runs(run_id) ON DELETE CASCADE,
    parent_artifact_id   UUID REFERENCES config_linkage_audit_artifacts(artifact_id) ON DELETE RESTRICT,
    metadata_json        TEXT NOT NULL DEFAULT '{}',
    created_at           BIGINT NOT NULL
);

CREATE INDEX idx_config_linkage_workspace
    ON config_linkage_audit_artifacts(workspace_id, created_at DESC);
CREATE INDEX idx_config_linkage_config_version
    ON config_linkage_audit_artifacts(config_version_id, created_at DESC);
CREATE UNIQUE INDEX uq_config_linkage_run_artifact
    ON config_linkage_audit_artifacts(run_id)
    WHERE run_id IS NOT NULL;

CREATE OR REPLACE FUNCTION reject_config_linkage_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'config_linkage_audit_artifacts is append-only';
END;
$$;

CREATE TRIGGER trg_config_linkage_no_update
    BEFORE UPDATE ON config_linkage_audit_artifacts
    FOR EACH ROW EXECUTE FUNCTION reject_config_linkage_mutation();

CREATE TRIGGER trg_config_linkage_no_delete
    BEFORE DELETE ON config_linkage_audit_artifacts
    FOR EACH ROW EXECUTE FUNCTION reject_config_linkage_mutation();

CREATE POLICY config_linkage_select_tenant ON config_linkage_audit_artifacts
    FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true));
CREATE POLICY config_linkage_insert_tenant ON config_linkage_audit_artifacts
    FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true));

GRANT SELECT, INSERT ON config_linkage_audit_artifacts TO api_accessor;
GRANT SELECT ON config_linkage_audit_artifacts TO worker_accessor;
