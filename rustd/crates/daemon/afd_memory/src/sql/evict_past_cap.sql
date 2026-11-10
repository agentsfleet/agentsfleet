DELETE FROM memory.entries
WHERE fleet_id = $1
  AND id IN (
    SELECT id FROM memory.entries
    WHERE fleet_id = $1
    ORDER BY (category = $3) DESC, updated_at DESC, id DESC
    OFFSET $2
  )
