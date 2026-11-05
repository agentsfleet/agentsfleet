-- Immutable publication authority exists before any branch mutation.
CREATE TABLE fleet.task_effects (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_effect_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    source_item_id UUID NOT NULL,
    original_lease_id UUID NOT NULL,
    original_runner_id UUID NOT NULL,
    original_fence BIGINT NOT NULL,
    task_generation BIGINT NOT NULL CHECK (task_generation > 0),
    stop_generation BIGINT NOT NULL CHECK (stop_generation >= 0),
    ordinal BIGINT NOT NULL CHECK (ordinal > 0),
    operation_id UUID NOT NULL,
    covered_input_sequence BIGINT NOT NULL CHECK (covered_input_sequence >= 0),
    expected_head TEXT NOT NULL,
    tested_tree TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    repository_id BIGINT NOT NULL,
    installation_id BIGINT NOT NULL,
    issuance_expires_at TIMESTAMPTZ(3) NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_task_effects_ordinal UNIQUE (task_id, ordinal),
    CONSTRAINT uq_task_effects_operation UNIQUE (task_id, operation_id)
);
CREATE INDEX idx_task_effects_source_item ON fleet.task_effects (source_item_id);
CREATE FUNCTION fleet.task_effects_refuse_update() RETURNS trigger AS $$
BEGIN RAISE EXCEPTION 'task_effects is append-only'; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER trg_task_effects_refuse_update BEFORE UPDATE ON fleet.task_effects
    FOR EACH ROW EXECUTE FUNCTION fleet.task_effects_refuse_update();
-- Opaque effect identity and receipt survive privacy deletion, like decision usage.
GRANT SELECT, INSERT ON fleet.task_effects TO api_runtime;
