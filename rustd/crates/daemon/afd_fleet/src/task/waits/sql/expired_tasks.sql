WITH wall AS MATERIALIZED (
    SELECT id AS task_id, fleet_id, workspace_id
    FROM fleet.tasks WHERE terminal_at IS NULL AND expires_at <= statement_timestamp()
    ORDER BY expires_at, id LIMIT $1
), attempts AS MATERIALIZED (
    SELECT t.id AS task_id, t.fleet_id, t.workspace_id
    FROM fleet.tasks t WHERE t.terminal_at IS NULL AND t.used_runs >= t.max_runs
      AND NOT EXISTS (SELECT 1 FROM thread.runs l WHERE l.task_id = t.id
          AND l.status = $2 AND l.run_expires_at > statement_timestamp())
    ORDER BY t.id LIMIT $1
)
SELECT task_id, fleet_id, workspace_id FROM wall
UNION
SELECT task_id, fleet_id, workspace_id FROM attempts
ORDER BY task_id LIMIT $1
