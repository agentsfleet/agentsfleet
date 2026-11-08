UPDATE fleet.admissions SET delivered_at = COALESCE(delivered_at, clock_timestamp()), updated_at = now()
WHERE fleet_id = $1 AND id = $2 AND lease_fence = $3 AND settled_at IS NULL
