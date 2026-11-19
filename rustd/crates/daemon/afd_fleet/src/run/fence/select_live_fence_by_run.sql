SELECT l.fencing_token AS own, a.fencing_token AS live_seq
FROM thread.runs l
JOIN thread.threads a ON a.fleet_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2 AND l.fleet_id = $3
  AND l.status = $4 AND l.run_expires_at > statement_timestamp()
LIMIT 1
