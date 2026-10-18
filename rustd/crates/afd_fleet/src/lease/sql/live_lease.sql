SELECT l.fleet_id::text AS fleet_id, l.workspace_id::text AS workspace_id, e.event_id,
       l.fencing_token AS fence, COALESCE(a.fencing_token, l.fencing_token) AS live_seq,
       e.actor
FROM runner.leases l
JOIN fleet.events e ON e.id = l.fleet_event_id
LEFT JOIN runner.fleet_slots a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > clock_timestamp()
