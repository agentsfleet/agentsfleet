UPDATE fleet.tasks t SET status = $6, status_reason = $7,
    generation = generation + 1, stop_generation = stop_generation + 1,
    resource_version = resource_version + 1, updated_at = now()
FROM thread.turns a JOIN thread.threads s ON s.fleet_id = a.thread_id
WHERE a.id = $1 AND a.thread_id = $2 AND a.workspace_id = $3
  AND a.run_fence = $4 AND s.fencing_token = $4 AND s.run_expires_at > clock_timestamp()
  AND t.id = a.task_id AND t.generation = a.task_generation AND t.terminal_at IS NULL
  AND t.status IN ($5, $8)
RETURNING t.id, t.resource_version
