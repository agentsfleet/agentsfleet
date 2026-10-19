SELECT l.fleet_id::text AS fleet_id, e.event_id, l.created_at, l.event_created_at,
       l.status, l.lease_expires_at, l.fencing_token,
       l.status = $3 AND l.lease_expires_at > statement_timestamp() AS live
FROM runner.leases l
JOIN fleet.events e ON e.id = l.fleet_event_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
