-- Terminal GitHub production evidence is retained before repair correlation.

CREATE TABLE IF NOT EXISTS core.repair_production_results (
    id                     UUID   PRIMARY KEY,
    CONSTRAINT ck_repair_production_results_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id           UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    provider               TEXT   NOT NULL,
    provider_deployment_id TEXT   NOT NULL,
    provider_status_id     TEXT   NOT NULL,
    repository             TEXT   NOT NULL,
    environment            TEXT   NOT NULL,
    commit_sha             TEXT   NOT NULL,
    conclusion             TEXT   NOT NULL,
    completed_at           BIGINT NOT NULL,
    created_at             BIGINT NOT NULL,
    CONSTRAINT uq_repair_production_results_provider
        UNIQUE (workspace_id, provider, provider_status_id)
);

CREATE INDEX IF NOT EXISTS idx_repair_production_results_workspace_repo_commit
    ON core.repair_production_results (workspace_id, lower(repository), commit_sha, id);

CREATE OR REPLACE FUNCTION core.repair_production_results_append_only() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' AND current_setting('fleet.allow_gate_purge', true) = 'on' THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'repair_production_results is append-only';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_repair_production_results_append_only ON core.repair_production_results;
CREATE TRIGGER trg_repair_production_results_append_only
    BEFORE UPDATE OR DELETE ON core.repair_production_results
    FOR EACH ROW EXECUTE FUNCTION core.repair_production_results_append_only();

GRANT SELECT, INSERT ON core.repair_production_results TO api_runtime;
