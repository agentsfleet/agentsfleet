WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
)
SELECT l.workspace_id, l.fleet_id,
       r.definition::text AS definition, CASE WHEN issued.id <> r.id THEN issued.definition::text END AS issued_definition,
       COALESCE(issued.id = r.id, FALSE) AS issued_is_current, l.event_id
FROM runner.runs l
CROSS JOIN instant
JOIN fleet.fleets f ON f.id = l.fleet_id
JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
LEFT JOIN fleet.tasks task ON task.id = l.task_id AND task.fleet_id = f.id
LEFT JOIN fleet.revisions issued
  ON issued.id = COALESCE(task.accepted_revision_id, l.setup_revision_id) AND issued.fleet_id = f.id
LEFT JOIN runner.slots a ON a.fleet_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2
  AND l.status = $3 AND l.cancel_requested_at IS NULL AND l.run_expires_at > instant.read_at
  AND l.fencing_token >= COALESCE(a.fencing_token, l.fencing_token)
