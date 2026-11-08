DELETE FROM memory.entries
WHERE fleet_id = $1 AND key = $2
RETURNING key
