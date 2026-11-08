UPDATE fleet.admissions SET settled_at = COALESCE(settled_at, clock_timestamp()), parked_at = NULL,
    updated_at = clock_timestamp()
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid AND interpreted_at IS NOT NULL
