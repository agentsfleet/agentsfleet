-- The lease freezes semantic ownership and meters its own task-running delta.
-- Opaque task_id already outlives task deletion; these add no privacy owner FK.
ALTER TABLE runner.leases
    ADD COLUMN task_generation BIGINT CHECK (task_generation > 0),
    ADD COLUMN task_stop_generation BIGINT CHECK (task_stop_generation >= 0),
    ADD COLUMN task_started_at TIMESTAMPTZ(3),
    ADD COLUMN task_runtime_accounted_at TIMESTAMPTZ(3),
    ADD COLUMN task_execution_deadline TIMESTAMPTZ(3);
CREATE INDEX idx_runner_leases_task ON runner.leases (task_id) WHERE task_id IS NOT NULL;
