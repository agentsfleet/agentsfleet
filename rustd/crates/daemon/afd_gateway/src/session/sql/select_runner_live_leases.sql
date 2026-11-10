-- Reconnect/recheck only: no activity frame or ordinary scheduling pass runs this read.
-- Both branches use (runner_id, status, lease_expires_at) index prefixes.
-- UNION ALL preserves the rows without a deduplication sort; fleet_slots uses its primary key.
SELECT l.id AS lease_id, l.lease_expires_at AS expires_at,
       l.fleet_id, l.event_id, l.created_at, l.event_created_at,
       l.fencing_token, l.cancel_requested_at IS NOT NULL AS cancelled
FROM runner.leases l
JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id
WHERE l.runner_id = $1 AND l.status = $2
  AND l.lease_expires_at > statement_timestamp()
  AND s.last_runner_id = l.runner_id AND s.fencing_token = l.fencing_token
  AND s.lease_expires_at > statement_timestamp()
UNION ALL
SELECT a.lease_id, LEAST(a.lease_expires_at, a.expires_at),
       NULL, NULL, a.created_at, NULL, a.fencing_token, false
FROM authoring.attempts a
WHERE a.runner_id = $1 AND a.status = $2
  AND a.lease_expires_at > statement_timestamp() AND a.expires_at > statement_timestamp()
LIMIT $3
