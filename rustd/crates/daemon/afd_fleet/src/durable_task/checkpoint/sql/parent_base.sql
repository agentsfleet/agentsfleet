-- The retained base of task $2's ready generation $1, which a delta plan names as its parent.
SELECT checkpoint_base_id FROM runner.checkpoints
WHERE id = $1 AND durable_task_id = $2 AND ready_at IS NOT NULL AND mode = $3
