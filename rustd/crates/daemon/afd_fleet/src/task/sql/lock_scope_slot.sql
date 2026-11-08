SELECT fleet_id::text FROM runner.fleet_slots
WHERE fleet_id = $1::uuid FOR UPDATE
