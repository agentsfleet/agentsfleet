SELECT id, run_fence, durable_task_id, durable_task_generation,
       settled_at IS NOT NULL AS settled
FROM thread.turns
WHERE id = $1 AND thread_id = $2 AND workspace_id = $3 AND producer IS NOT NULL
FOR UPDATE
