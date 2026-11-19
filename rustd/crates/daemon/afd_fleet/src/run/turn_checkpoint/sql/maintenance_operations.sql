WITH expired AS MATERIALIZED (
    SELECT turn_checkpoint_id FROM runner.turn_checkpoint_operations
    WHERE turn_id=$1 ORDER BY created_at,turn_checkpoint_id LIMIT $2
)
DELETE FROM runner.turn_checkpoint_operations op USING expired e
WHERE op.turn_checkpoint_id=e.turn_checkpoint_id
