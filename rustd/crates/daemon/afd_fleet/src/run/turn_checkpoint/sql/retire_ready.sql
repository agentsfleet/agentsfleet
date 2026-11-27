-- Retires turn $1's newest unpinned ready generation that is not the head $2 and that no live
-- plan is a delta against, with the abandoned plans that were; a superseded chain therefore goes
-- newest first.
WITH candidate AS MATERIALIZED (
    SELECT p.id
    FROM runner.checkpoints p
    WHERE p.turn_id = $1 AND p.id IS DISTINCT FROM $2 AND p.ready_at IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM runner.checkpoints child
          WHERE child.parent_checkpoint_id = p.id AND child.abandoned_at IS NULL
      )
    ORDER BY p.generation DESC
    LIMIT 1
    FOR UPDATE OF p
), checked AS MATERIALIZED (
    SELECT c.id
    FROM candidate c
    WHERE NOT EXISTS (
        SELECT 1 FROM runner.checkpoint_pins pin
        WHERE pin.checkpoint_id = c.id AND pin.released_at IS NULL
          AND pin.expires_at > clock_timestamp() AND pin.operation_expires_at > clock_timestamp()
    )
), removed AS (
    DELETE FROM runner.checkpoints p
    USING checked c
    WHERE p.id = c.id OR (p.parent_checkpoint_id = c.id AND p.abandoned_at IS NOT NULL)
    RETURNING p.id
)
SELECT r.id FROM removed r JOIN checked c ON c.id = r.id
