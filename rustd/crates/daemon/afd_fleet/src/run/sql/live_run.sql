SELECT l.fleet_id AS fleet_id, l.workspace_id AS workspace_id, e.id AS turn_id,
       l.fencing_token AS fence, COALESCE(a.fencing_token, l.fencing_token) AS live_seq,
       e.actor, l.run_expires_at
FROM thread.runs l
JOIN thread.turns e ON e.id = l.turn_id AND e.thread_id = l.fleet_id
LEFT JOIN thread.threads a ON a.fleet_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
