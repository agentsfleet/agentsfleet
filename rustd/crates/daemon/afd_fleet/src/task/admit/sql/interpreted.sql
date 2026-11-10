UPDATE fleet.admissions SET interpreted_at = clock_timestamp(), updated_at = clock_timestamp()
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3 AND interpreted_at IS NULL
