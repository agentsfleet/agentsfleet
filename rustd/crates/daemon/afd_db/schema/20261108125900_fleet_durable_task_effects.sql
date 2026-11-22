-- Append-only publication authority, recorded before any branch mutation. Ids are opaque so the
-- effect and its receipt survive privacy deletion.
CREATE TABLE fleet.durable_task_effects (
    id UUID PRIMARY KEY,
    durable_task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    source_item_id UUID NOT NULL,
    original_run_id UUID NOT NULL,
    original_runner_id UUID NOT NULL,
    original_fence BIGINT NOT NULL,
    durable_task_generation BIGINT NOT NULL CHECK (durable_task_generation > 0),
    stop_generation BIGINT NOT NULL CHECK (stop_generation >= 0),
    ordinal BIGINT NOT NULL CHECK (ordinal > 0),
    operation_id UUID NOT NULL,
    covered_input_sequence BIGINT NOT NULL CHECK (covered_input_sequence >= 0),
    expected_head TEXT NOT NULL,
    tested_tree TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    github_repository_id BIGINT NOT NULL,
    github_installation_id BIGINT NOT NULL,
    issuance_expires_at TIMESTAMPTZ(3) NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_durable_task_effects_ordinal UNIQUE (durable_task_id, ordinal),
    CONSTRAINT uq_durable_task_effects_operation UNIQUE (durable_task_id, operation_id)
);

CREATE FUNCTION fleet.durable_task_effects_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'durable_task_effects is append-only'; END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_durable_task_effects_refuse_update BEFORE UPDATE ON fleet.durable_task_effects
    FOR EACH ROW EXECUTE FUNCTION fleet.durable_task_effects_refuse_update();

GRANT SELECT, INSERT ON fleet.durable_task_effects TO api_runtime;
