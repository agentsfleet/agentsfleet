SELECT l.fleet_id AS fleet_id, l.workspace_id AS workspace_id, e.event_id AS event_id,
       l.fencing_token AS fence, COALESCE(a.fencing_token, l.fencing_token) AS live_seq,
       e.actor, l.lease_expires_at
FROM runner.leases l
JOIN fleet.events e ON e.event_id = l.event_id
LEFT JOIN runner.fleet_slots a ON a.fleet_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
