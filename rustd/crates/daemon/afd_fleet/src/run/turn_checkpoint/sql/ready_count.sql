-- Turn $1's ready generations, its head $2's delta chain counted once, as the head.
WITH RECURSIVE chain (id) AS (
    SELECT parent_checkpoint_id FROM runner.checkpoints
    WHERE id = $2 AND parent_checkpoint_id IS NOT NULL
    UNION ALL
    SELECT c.parent_checkpoint_id
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT count(*)
FROM runner.checkpoints
WHERE turn_id = $1 AND ready_at IS NOT NULL AND id NOT IN (SELECT id FROM chain)
