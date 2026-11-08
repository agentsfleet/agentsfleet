SELECT fleet_id FROM runner.fleet_slots
WHERE fleet_id = $1 FOR UPDATE
