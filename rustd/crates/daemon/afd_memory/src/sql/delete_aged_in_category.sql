DELETE FROM memory.entries
WHERE fleet_id = $1
  AND category = $2
  AND updated_at < $3
