WITH candidate AS MATERIALIZED (
    SELECT p.id
    FROM runner.turn_checkpoints p
    WHERE p.turn_id = $1 AND p.id IS DISTINCT FROM $2 AND p.ready_at IS NOT NULL
    ORDER BY p.generation
    LIMIT 1
    FOR UPDATE OF p
), checked AS MATERIALIZED (
    SELECT id, clock_timestamp() AS checked_at FROM candidate
)
DELETE FROM runner.turn_checkpoints p
USING checked c
WHERE p.id = c.id
  AND NOT EXISTS (
      SELECT 1 FROM runner.turn_checkpoint_pins pin
      WHERE pin.turn_checkpoint_id = p.id AND pin.released_at IS NULL
        AND pin.expires_at > c.checked_at AND pin.operation_expires_at > c.checked_at
  )
RETURNING p.id
