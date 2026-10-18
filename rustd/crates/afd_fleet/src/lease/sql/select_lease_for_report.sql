SELECT l.fleet_id::text AS fleet_id, l.workspace_id::text AS workspace_id,
       l.tenant_id::text AS tenant_id, e.event_id, e.actor, l.posture, l.provider,
       l.model, l.fencing_token, l.receipt
FROM runner.leases l
JOIN fleet.events e ON e.id = l.fleet_event_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
