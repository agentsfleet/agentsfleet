-- The live runs a purged fleet held go with it, as the cascade this replaces
-- did; its settled runs stay as runner history (deletion matrix: Fleet).
DELETE FROM thread.runs
WHERE fleet_id = $1 AND status = $2
