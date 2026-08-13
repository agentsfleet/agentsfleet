-- Immutable evidence for every completed workflow on an approved repair
-- branch. A run may arrive before the Pull Request opens, so the gate-resolved
-- Fleet, event, repository, and branch are recorded without a link-row key.

CREATE TABLE IF NOT EXISTS core.repair_run_results (
    id                UUID   PRIMARY KEY,
    CONSTRAINT ck_repair_run_results_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id      UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    fleet_id          UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    event_id          TEXT   NOT NULL,
    repository        TEXT   NOT NULL,
    branch            TEXT   NOT NULL,
    workflow_name     TEXT   NOT NULL,
    provider_run_id   BIGINT NOT NULL,
    head_commit_sha   TEXT   NOT NULL,
    conclusion        TEXT   NOT NULL,
    completed_at      BIGINT NOT NULL,
    created_at        BIGINT NOT NULL,
    CONSTRAINT fk_repair_run_results_event
        FOREIGN KEY (fleet_id, event_id)
        REFERENCES core.fleet_events(fleet_id, event_id) ON DELETE CASCADE,
    CONSTRAINT uq_repair_run_results_provider_run
        UNIQUE (fleet_id, repository, provider_run_id)
);

CREATE INDEX IF NOT EXISTS idx_repair_run_results_workspace
    ON core.repair_run_results (workspace_id);

CREATE INDEX IF NOT EXISTS idx_repair_run_results_fleet_event
    ON core.repair_run_results (fleet_id, event_id);

CREATE OR REPLACE FUNCTION core.repair_run_results_frozen() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' AND current_setting('fleet.allow_gate_purge', true) = 'on' THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'repair_run_results is append-only';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_repair_run_results_frozen ON core.repair_run_results;
CREATE TRIGGER trg_repair_run_results_frozen
    BEFORE UPDATE OR DELETE ON core.repair_run_results
    FOR EACH ROW EXECUTE FUNCTION core.repair_run_results_frozen();

GRANT SELECT, INSERT ON core.repair_run_results TO api_runtime;
