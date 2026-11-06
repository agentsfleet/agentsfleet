SELECT actor, event_type, request_json FROM fleet.admissions
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid AND interpreted_at IS NOT NULL
FOR UPDATE
