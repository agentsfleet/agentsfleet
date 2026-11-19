SELECT fleet_id FROM runner.slots
WHERE fleet_id = $1 FOR UPDATE
