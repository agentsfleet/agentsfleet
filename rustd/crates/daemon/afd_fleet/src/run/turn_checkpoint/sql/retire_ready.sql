WITH candidate AS MATERIALIZED (
    SELECT p.id
    FROM runner.checkpoints p
    WHERE p.turn_id = $1 AND p.id IS DISTINCT FROM $2 AND p.ready_at IS NOT NULL
    ORDER BY p.generation
    LIMIT 1
    FOR UPDATE OF p
), checked AS MATERIALIZED (
    SELECT id, clock_timestamp() AS checked_at FROM candidate
)
DELETE FROM runner.checkpoints p
USING checked c
WHERE p.id = c.id
  AND NOT EXISTS (
      SELECT 1 FROM runner.checkpoint_pins pin
      WHERE pin.checkpoint_id = p.id AND pin.released_at IS NULL
        AND pin.expires_at > c.checked_at AND pin.operation_expires_at > c.checked_at
  )
RETURNING p.id
