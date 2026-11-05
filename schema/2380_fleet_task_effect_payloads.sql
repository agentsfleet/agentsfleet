-- Private repository/branch/workflow details purge with their native task owner.
CREATE TABLE fleet.task_effect_payloads (
    effect_id UUID PRIMARY KEY REFERENCES fleet.task_effects (id),
    task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    target JSONB NOT NULL,
    request JSONB NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_effect_payloads_task FOREIGN KEY (task_id, fleet_id, workspace_id)
        REFERENCES fleet.tasks (id, fleet_id, workspace_id) ON DELETE CASCADE
);
CREATE INDEX idx_effect_payloads_task ON fleet.task_effect_payloads (task_id);
CREATE FUNCTION fleet.task_effect_payloads_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'task_effect_payloads is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_task_effect_payloads_refuse_update BEFORE UPDATE ON fleet.task_effect_payloads
    FOR EACH ROW EXECUTE FUNCTION fleet.task_effect_payloads_refuse_update();
GRANT SELECT, INSERT, DELETE ON fleet.task_effect_payloads TO api_runtime;
