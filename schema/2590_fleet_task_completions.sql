-- Only the verified GitHub adapter writes this short-lived completion inbox.
-- It closes event-before-arm races; conclusions never complete a goal directly.
CREATE TABLE fleet.task_completions (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_completion_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    scope_fleet_id UUID REFERENCES fleet.fleets (id) ON DELETE CASCADE,
    installation_id BIGINT NOT NULL CHECK (installation_id > 0),
    repository_id BIGINT NOT NULL CHECK (repository_id > 0),
    head_oid TEXT NOT NULL,
    workflow_id BIGINT NOT NULL CHECK (workflow_id > 0),
    run_id BIGINT NOT NULL CHECK (run_id > 0),
    run_number BIGINT NOT NULL CHECK (run_number > 0),
    run_attempt INTEGER NOT NULL CHECK (run_attempt > 0),
    conclusion TEXT NOT NULL,
    body_digest TEXT NOT NULL,
    received_at TIMESTAMPTZ(3) NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_task_completion_run UNIQUE NULLS NOT DISTINCT (workspace_id, scope_fleet_id, installation_id, repository_id, run_id, run_attempt)
);
CREATE INDEX idx_task_completion_match ON fleet.task_completions
    (workspace_id, installation_id, repository_id, head_oid, workflow_id, run_id, run_attempt);
CREATE INDEX idx_task_completion_fleet ON fleet.task_completions (scope_fleet_id) WHERE scope_fleet_id IS NOT NULL;
CREATE INDEX idx_task_completion_retention ON fleet.task_completions (received_at, id);
CREATE FUNCTION fleet.task_completions_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'task_completions is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_task_completions_refuse_update BEFORE UPDATE ON fleet.task_completions
    FOR EACH ROW EXECUTE FUNCTION fleet.task_completions_refuse_update();
GRANT SELECT, INSERT, DELETE ON fleet.task_completions TO api_runtime;
