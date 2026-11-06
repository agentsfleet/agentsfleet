-- One renewable native pin protects a capture/restore object's reference set.
CREATE TABLE fleet.task_object_pins (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_pin_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    task_checkpoint_id UUID NOT NULL,
    task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    kind TEXT NOT NULL,
    owner_lease_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL,
    task_generation BIGINT NOT NULL,
    stop_generation BIGINT NOT NULL,
    resource_version BIGINT NOT NULL CHECK (resource_version > 0),
    expires_at TIMESTAMPTZ(3) NOT NULL,
    operation_expires_at TIMESTAMPTZ(3) NOT NULL,
    released_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_task_pins_checkpoint FOREIGN KEY (task_checkpoint_id, task_id, fleet_id, workspace_id)
        REFERENCES fleet.task_checkpoints (id, task_id, fleet_id, workspace_id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX uq_task_object_pin_live ON fleet.task_object_pins (task_checkpoint_id, task_id, kind)
    WHERE released_at IS NULL;
CREATE INDEX idx_task_object_pins_task ON fleet.task_object_pins (task_id);
CREATE INDEX idx_task_object_pins_expiry ON fleet.task_object_pins (expires_at, id) WHERE released_at IS NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_object_pins TO api_runtime;
