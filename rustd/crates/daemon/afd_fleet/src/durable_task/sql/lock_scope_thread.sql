SELECT fleet_id FROM thread.threads
WHERE fleet_id = $1 FOR UPDATE
