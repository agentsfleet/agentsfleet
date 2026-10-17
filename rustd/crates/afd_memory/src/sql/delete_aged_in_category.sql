DELETE FROM memory.entries
WHERE fleet_id = $1::uuid
  AND category = $2
  AND updated_at < $3
