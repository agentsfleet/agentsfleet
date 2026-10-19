DELETE FROM memory.entries
WHERE fleet_id = $1::uuid
  AND id IN (
    SELECT id FROM memory.entries
    WHERE fleet_id = $1::uuid
    ORDER BY (category = $3) DESC, updated_at DESC, id DESC
    OFFSET $2
  )
