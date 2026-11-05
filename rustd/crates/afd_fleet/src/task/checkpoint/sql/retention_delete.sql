DELETE FROM fleet.task_checkpoints c USING fleet.tasks t
WHERE c.task_id = $1::uuid AND t.id = c.task_id
  AND NOT EXISTS (SELECT 1 FROM fleet.task_object_pins p
      WHERE p.task_checkpoint_id = c.id AND p.released_at IS NULL AND p.expires_at > clock_timestamp())
  AND (c.abandoned_at IS NOT NULL
       OR (t.terminal_at IS NOT NULL AND t.terminal_at <= clock_timestamp() - make_interval(secs => $3))
       OR (c.ready_at IS NOT NULL AND c.id IS DISTINCT FROM t.task_checkpoint_id
           AND c.id IN (SELECT old.id FROM fleet.task_checkpoints old
               WHERE old.task_id = t.id AND old.ready_at IS NOT NULL
               ORDER BY old.generation DESC OFFSET $2)))
