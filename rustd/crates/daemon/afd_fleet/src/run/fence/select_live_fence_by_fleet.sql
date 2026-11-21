SELECT l.fencing_token AS own, a.fencing_token AS live_seq
FROM thread.runs l
JOIN thread.threads a ON a.fleet_id = l.fleet_id
WHERE l.runner_id = $1 AND l.fleet_id = $2
  AND l.status = $3 AND l.run_expires_at > statement_timestamp()
ORDER BY l.fencing_token DESC
LIMIT 1
