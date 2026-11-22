-- A writer's or reader's bounded custody of one checkpoint while it uploads or restores. A pin's
-- deadline renews but never passes its operation's (`operation_expires_at`). A turn pin keys its
-- reader by operation (`operation_id`); a task pin carries the task generations its guard
-- compares; exactly one of the two shapes. A turn allows one live pin per run and kind, a task
-- one per checkpoint and kind.
CREATE TABLE runner.checkpoint_pins (
    id UUID PRIMARY KEY,
    checkpoint_id UUID NOT NULL REFERENCES runner.checkpoints (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    operation_id UUID,
    owner_run_id UUID NOT NULL,
    owner_fence BIGINT NOT NULL CHECK (owner_fence > 0),
    durable_task_generation BIGINT CHECK (durable_task_generation > 0),
    stop_generation BIGINT CHECK (stop_generation >= 0),
    resource_version BIGINT NOT NULL CHECK (resource_version > 0),
    expires_at TIMESTAMPTZ(3) NOT NULL,
    operation_expires_at TIMESTAMPTZ(3) NOT NULL,
    released_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_checkpoint_pins_shape CHECK (num_nonnulls(operation_id, durable_task_generation) = 1
        AND (durable_task_generation IS NULL) = (stop_generation IS NULL)),
    CONSTRAINT ck_checkpoint_pins_deadline CHECK (expires_at <= operation_expires_at)
);

CREATE UNIQUE INDEX uq_checkpoint_pins_live ON runner.checkpoint_pins (checkpoint_id, owner_run_id, kind)
    WHERE released_at IS NULL;
CREATE UNIQUE INDEX uq_checkpoint_pins_durable_task_live ON runner.checkpoint_pins (checkpoint_id, kind)
    WHERE released_at IS NULL AND durable_task_generation IS NOT NULL;
CREATE UNIQUE INDEX uq_checkpoint_pins_operation ON runner.checkpoint_pins
    (checkpoint_id, owner_run_id, kind, operation_id);
CREATE INDEX idx_checkpoint_pins_expiry ON runner.checkpoint_pins (expires_at, id)
    WHERE released_at IS NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.checkpoint_pins TO api_runtime;
