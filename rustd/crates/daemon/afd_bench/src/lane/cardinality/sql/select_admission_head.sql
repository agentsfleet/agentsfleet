SELECT id::text FROM fleet.admissions WHERE fleet_id = $1::uuid AND settled_at IS NULL ORDER BY created_at, sequence LIMIT 1
