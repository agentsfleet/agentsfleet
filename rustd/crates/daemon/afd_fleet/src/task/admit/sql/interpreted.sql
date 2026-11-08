UPDATE fleet.admissions SET interpreted_at = clock_timestamp(), updated_at = clock_timestamp()
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid AND interpreted_at IS NULL
