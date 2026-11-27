-- Removes task $1's obsolete generations: abandoned plans, every unpinned generation of a task
-- terminal for $3 seconds, and ready generations older than its newest $2 other than its pointer.
-- A generation a surviving delta is applied over stays, so no delete leaves a delta's parent
-- missing.
WITH RECURSIVE doomed AS MATERIALIZED (
    SELECT c.id
    FROM runner.checkpoints c JOIN fleet.durable_tasks t ON t.id = c.durable_task_id
    WHERE c.durable_task_id = $1
      AND NOT EXISTS (SELECT 1 FROM runner.checkpoint_pins p
          WHERE p.checkpoint_id = c.id AND p.released_at IS NULL AND p.expires_at > clock_timestamp())
      AND (c.abandoned_at IS NOT NULL
           OR (t.terminal_at IS NOT NULL AND t.terminal_at <= clock_timestamp() - make_interval(secs => $3))
           OR (c.ready_at IS NOT NULL AND c.id IS DISTINCT FROM t.checkpoint_id
               AND c.id IN (SELECT old.id FROM runner.checkpoints old
                   WHERE old.durable_task_id = t.id AND old.ready_at IS NOT NULL
                   ORDER BY old.generation DESC OFFSET $2)))
), kept (id) AS (
    SELECT c.parent_checkpoint_id FROM runner.checkpoints c
    WHERE c.durable_task_id = $1 AND c.parent_checkpoint_id IS NOT NULL
      AND c.id NOT IN (SELECT id FROM doomed)
    UNION
    SELECT c.parent_checkpoint_id FROM runner.checkpoints c JOIN kept k ON c.id = k.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
DELETE FROM runner.checkpoints c
WHERE c.id IN (SELECT id FROM doomed) AND c.id NOT IN (SELECT id FROM kept)
