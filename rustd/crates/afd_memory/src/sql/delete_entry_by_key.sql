DELETE FROM memory.entries
WHERE fleet_id = $1::uuid AND key = $2
RETURNING key
