-- The task an issued turn belongs to, its current generation, and whether the host runs tasks.
--
-- $1 turn · $2 fleet · $3 runner · $4 the task capability
SELECT a.durable_task_id, a.durable_task_generation AS turn_generation, t.generation,
       (r.durable_tasks_enabled AND COALESCE(r.capability_report @> jsonb_build_object($4::text, true), false)) AS capable
FROM thread.turns a
JOIN fleet.fleets f ON f.id = a.thread_id AND f.workspace_id = a.workspace_id
JOIN runner.runners r ON r.id = $3
LEFT JOIN fleet.durable_tasks t ON t.id = a.durable_task_id AND t.fleet_id = a.thread_id AND t.workspace_id = a.workspace_id
WHERE a.id = $1 AND a.thread_id = $2 AND a.settled_at IS NULL
