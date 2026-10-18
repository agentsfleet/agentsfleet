SELECT id::text AS event_id, fleet_id::text AS fleet_id, workspace_id::text AS workspace_id,
       actor, event_type, request_json, created_at
FROM fleet.admissions
WHERE fleet_id = $2::uuid AND workspace_id = $1::uuid AND producer = $3
  AND delivered_at IS NULL
ORDER BY created_at DESC, id DESC
LIMIT $4
