-- What one run authorises a mint to reach: its workspace, its fleet and its turn, while the run
-- is this runner's, live, not cancelled, and still holds its fleet's thread.
--
-- $1 run · $2 runner · $3 the active run status
WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
)
SELECT l.workspace_id, l.fleet_id, l.turn_id
FROM thread.runs l
CROSS JOIN instant
LEFT JOIN thread.threads a ON a.fleet_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2
  AND l.status = $3 AND l.cancel_requested_at IS NULL AND l.run_expires_at > instant.read_at
  AND l.fencing_token >= COALESCE(a.fencing_token, l.fencing_token)
