SELECT l.fleet_id, l.workspace_id,
       l.tenant_id, l.event_id, e.actor, l.posture, l.provider,
       l.model, l.fencing_token, l.created_at, l.event_created_at,
       -- The catalogue version the report prices at: its book comes from the
       -- replica's cache under this version, and the report statement guards
       -- on it.
       (SELECT c.resource_version FROM model.catalogue_version c WHERE c.id = 1) AS catalogue_version
FROM thread.runs l
JOIN thread.turns e ON e.id = l.event_id AND e.thread_id = l.fleet_id
WHERE l.id = $1 AND l.runner_id = $2
