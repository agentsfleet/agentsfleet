WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
)
SELECT l.workspace_id::text AS workspace_id, l.fleet_id::text AS fleet_id,
       r.definition::text AS definition, e.event_id
FROM runner.leases l
CROSS JOIN instant
JOIN fleet.fleets f ON f.id = l.fleet_id
JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
JOIN fleet.events e ON e.id = l.fleet_event_id
LEFT JOIN runner.fleet_slots a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > instant.read_at
  AND l.fencing_token >= COALESCE(a.fencing_token, l.fencing_token)
