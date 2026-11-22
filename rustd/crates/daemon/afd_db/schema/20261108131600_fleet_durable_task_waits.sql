-- Native dependency waits own wakeup eligibility, not a second work queue.
CREATE TABLE fleet.durable_task_waits (
    id UUID PRIMARY KEY,
    durable_task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    owner_run_id UUID NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    expected_head TEXT NOT NULL,
    checks JSONB NOT NULL,
    cancelled_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_wait_scope FOREIGN KEY (durable_task_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT uq_durable_task_wait_owner UNIQUE (owner_run_id),
    CONSTRAINT uq_durable_task_wait_generation UNIQUE (durable_task_id, generation)
);

CREATE UNIQUE INDEX uq_durable_task_wait_current ON fleet.durable_task_waits (durable_task_id)
    WHERE cancelled_at IS NULL;

CREATE INDEX idx_durable_task_wait_fleet ON fleet.durable_task_waits (fleet_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.durable_task_waits TO api_runtime;
