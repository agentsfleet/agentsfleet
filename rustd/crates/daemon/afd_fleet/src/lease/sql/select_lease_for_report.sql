SELECT l.fleet_id, l.workspace_id,
       l.tenant_id, l.event_id, e.actor, l.posture, l.provider,
       l.model, l.fencing_token, l.created_at, l.event_created_at
FROM runner.leases l
JOIN fleet.events e ON e.event_id = l.event_id
WHERE l.id = $1 AND l.runner_id = $2
