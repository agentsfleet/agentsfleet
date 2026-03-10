-- UseZombie M5_008 immutable compile/activate/run linkage artifacts

CREATE TABLE IF NOT EXISTS profile_linkage_audit_artifacts (
    artifact_id          TEXT PRIMARY KEY,
    tenant_id            TEXT NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    workspace_id         TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    artifact_type        TEXT NOT NULL CHECK (artifact_type IN ('COMPILE', 'ACTIVATE', 'RUN')),
    profile_version_id   TEXT NOT NULL REFERENCES agent_profile_versions(profile_version_id) ON DELETE RESTRICT,
    compile_job_id       TEXT REFERENCES profile_compile_jobs(compile_job_id) ON DELETE RESTRICT,
    run_id               TEXT REFERENCES runs(run_id) ON DELETE CASCADE,
    parent_artifact_id   TEXT REFERENCES profile_linkage_audit_artifacts(artifact_id) ON DELETE RESTRICT,
    metadata_json        TEXT NOT NULL DEFAULT '{}',
    created_at           BIGINT NOT NULL,
    CHECK ((artifact_type = 'COMPILE' AND compile_job_id IS NOT NULL AND run_id IS NULL)
        OR (artifact_type = 'ACTIVATE' AND run_id IS NULL)
        OR (artifact_type = 'RUN' AND run_id IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_profile_linkage_workspace
    ON profile_linkage_audit_artifacts(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_profile_linkage_profile_version
    ON profile_linkage_audit_artifacts(profile_version_id, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS uq_profile_linkage_run_artifact
    ON profile_linkage_audit_artifacts(run_id)
    WHERE artifact_type = 'RUN';

CREATE OR REPLACE FUNCTION reject_profile_linkage_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'profile_linkage_audit_artifacts is append-only';
END;
$$;

DROP TRIGGER IF EXISTS trg_profile_linkage_no_update ON profile_linkage_audit_artifacts;
CREATE TRIGGER trg_profile_linkage_no_update
    BEFORE UPDATE ON profile_linkage_audit_artifacts
    FOR EACH ROW EXECUTE FUNCTION reject_profile_linkage_mutation();

DROP TRIGGER IF EXISTS trg_profile_linkage_no_delete ON profile_linkage_audit_artifacts;
CREATE TRIGGER trg_profile_linkage_no_delete
    BEFORE DELETE ON profile_linkage_audit_artifacts
    FOR EACH ROW EXECUTE FUNCTION reject_profile_linkage_mutation();

ALTER TABLE profile_linkage_audit_artifacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE profile_linkage_audit_artifacts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS profile_linkage_select_tenant ON profile_linkage_audit_artifacts;
DROP POLICY IF EXISTS profile_linkage_insert_tenant ON profile_linkage_audit_artifacts;
CREATE POLICY profile_linkage_select_tenant ON profile_linkage_audit_artifacts
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true));
CREATE POLICY profile_linkage_insert_tenant ON profile_linkage_audit_artifacts
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true));

GRANT SELECT, INSERT ON profile_linkage_audit_artifacts TO api_accessor;
GRANT SELECT ON profile_linkage_audit_artifacts TO worker_accessor;
