-- Complete private interpretation survives admission cleanup, until fleet purge.
CREATE TABLE fleet.task_interpretations (
    admission_id UUID PRIMARY KEY,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    plan_digest TEXT NOT NULL,
    message TEXT NOT NULL,
    plan JSONB NOT NULL,
    accepting_lease_id UUID NOT NULL,
    accepting_fence BIGINT NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_task_interpretations_fleet FOREIGN KEY (fleet_id, workspace_id, tenant_id)
        REFERENCES fleet.fleets (id, workspace_id, tenant_id) ON DELETE CASCADE
);
CREATE INDEX idx_task_interpretations_fleet ON fleet.task_interpretations (fleet_id);
CREATE FUNCTION fleet.task_interpretations_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'task_interpretations is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_task_interpretations_refuse_update BEFORE UPDATE ON fleet.task_interpretations
    FOR EACH ROW EXECUTE FUNCTION fleet.task_interpretations_refuse_update();
GRANT SELECT, INSERT, DELETE ON fleet.task_interpretations TO api_runtime;
