SELECT actor, event_type, request_json FROM fleet.admissions
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3 AND interpreted_at IS NOT NULL
FOR UPDATE
