UPDATE fleet.admissions SET settled_at = COALESCE(settled_at, clock_timestamp()), parked_at = NULL,
    updated_at = clock_timestamp()
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3 AND interpreted_at IS NOT NULL
