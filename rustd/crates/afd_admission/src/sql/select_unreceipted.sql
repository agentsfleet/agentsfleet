SELECT id::text AS id, fleet_id::text AS fleet_id, workspace_id::text AS workspace_id,
       actor, event_type, request_json, event_created_at
FROM fleet.admissions
WHERE receipt IS NULL AND created_at <= statement_timestamp() - $1::interval
ORDER BY created_at ASC, sequence ASC
LIMIT $2
FOR UPDATE SKIP LOCKED
