-- A short-lived inbox written only by the verified GitHub adapter. It closes event-before-arm
-- races; a conclusion never completes a goal directly.
CREATE TABLE fleet.durable_task_completions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES identity.workspaces (id) ON DELETE CASCADE,
    scope_fleet_id UUID REFERENCES fleet.fleets (id) ON DELETE CASCADE,
    github_installation_id BIGINT NOT NULL CHECK (github_installation_id > 0),
    github_repository_id BIGINT NOT NULL CHECK (github_repository_id > 0),
    head_oid TEXT NOT NULL,
    github_workflow_id BIGINT NOT NULL CHECK (github_workflow_id > 0),
    github_run_id BIGINT NOT NULL CHECK (github_run_id > 0),
    run_number BIGINT NOT NULL CHECK (run_number > 0),
    run_attempt INTEGER NOT NULL CHECK (run_attempt > 0),
    received_at TIMESTAMPTZ(3) NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_durable_task_completion_run UNIQUE NULLS NOT DISTINCT (workspace_id, scope_fleet_id, github_installation_id, github_repository_id, github_run_id, run_attempt)
);

CREATE INDEX idx_durable_task_completion_match ON fleet.durable_task_completions
    (workspace_id, github_installation_id, github_repository_id, head_oid, github_workflow_id, github_run_id, run_attempt);

CREATE INDEX idx_durable_task_completion_fleet ON fleet.durable_task_completions (scope_fleet_id) WHERE scope_fleet_id IS NOT NULL;

CREATE INDEX idx_durable_task_completion_retention ON fleet.durable_task_completions (received_at, id);

CREATE FUNCTION fleet.durable_task_completions_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'durable_task_completions is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_completions_refuse_update BEFORE UPDATE ON fleet.durable_task_completions
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_completions_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_completions TO api_runtime;
