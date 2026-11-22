-- Private repository, branch and workflow details; they purge with their task.
CREATE TABLE fleet.durable_task_effect_payloads (
    durable_task_effect_id UUID PRIMARY KEY REFERENCES fleet.durable_task_effects (id),
    durable_task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    target JSONB NOT NULL,
    request JSONB NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_effect_payloads_durable_task FOREIGN KEY (durable_task_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_tasks (id, fleet_id, workspace_id) ON DELETE CASCADE
);

CREATE INDEX idx_effect_payloads_durable_task ON fleet.durable_task_effect_payloads (durable_task_id);

CREATE FUNCTION fleet.durable_task_effect_payloads_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'durable_task_effect_payloads is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_effect_payloads_refuse_update BEFORE UPDATE ON fleet.durable_task_effect_payloads
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_effect_payloads_refuse_update();

GRANT SELECT, INSERT, DELETE ON fleet.durable_task_effect_payloads TO api_runtime;
