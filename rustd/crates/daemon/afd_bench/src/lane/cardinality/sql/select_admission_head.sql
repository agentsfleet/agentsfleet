SELECT id FROM fleet.admissions WHERE fleet_id = $1 AND settled_at IS NULL ORDER BY created_at, sequence LIMIT 1
