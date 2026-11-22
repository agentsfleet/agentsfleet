-- One renewable native pin protects a capture/restore object's reference set.
CREATE TABLE fleet.durable_task_object_pins (
    id UUID PRIMARY KEY,
    durable_task_checkpoint_id UUID NOT NULL,
    durable_task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    kind TEXT NOT NULL,
    owner_run_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL,
    durable_task_generation BIGINT NOT NULL,
    stop_generation BIGINT NOT NULL,
    resource_version BIGINT NOT NULL CHECK (resource_version > 0),
    expires_at TIMESTAMPTZ(3) NOT NULL,
    operation_expires_at TIMESTAMPTZ(3) NOT NULL,
    released_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_pins_checkpoint FOREIGN KEY (durable_task_checkpoint_id, durable_task_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_task_checkpoints (id, durable_task_id, fleet_id, workspace_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX uq_durable_task_object_pin_live ON fleet.durable_task_object_pins (durable_task_checkpoint_id, durable_task_id, kind)
    WHERE released_at IS NULL;

CREATE INDEX idx_durable_task_object_pins_durable_task ON fleet.durable_task_object_pins (durable_task_id);

CREATE INDEX idx_durable_task_object_pins_expiry ON fleet.durable_task_object_pins (expires_at, id) WHERE released_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.durable_task_object_pins TO api_runtime;
