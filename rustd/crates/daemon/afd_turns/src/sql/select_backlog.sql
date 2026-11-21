SELECT count(*) AS waiting,
       statement_timestamp() - min(a.created_at) AS oldest_age
FROM thread.turns a
-- Admitted fleet turns: a setup draft's turn has no producer.
WHERE a.settled_at IS NULL AND a.producer IS NOT NULL
  AND (a.producer <> $1 OR a.task_generation IS NULL OR EXISTS (
    SELECT 1 FROM fleet.tasks t WHERE t.id = a.task_id AND t.fleet_id = a.thread_id
      AND t.workspace_id = a.workspace_id AND t.generation = a.task_generation
      AND t.terminal_at IS NULL))
