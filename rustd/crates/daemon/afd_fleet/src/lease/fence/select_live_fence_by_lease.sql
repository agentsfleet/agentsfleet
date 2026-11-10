SELECT l.fencing_token AS own, COALESCE(a.fencing_token, l.fencing_token) AS live_seq
FROM runner.leases l
LEFT JOIN runner.fleet_slots a ON a.fleet_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2 AND l.fleet_id = $3
  AND l.status = $4 AND l.lease_expires_at > statement_timestamp()
LIMIT 1
