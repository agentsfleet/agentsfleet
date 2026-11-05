SELECT (SELECT count(*) FROM (
  SELECT 1 FROM fleet.admissions a
  WHERE a.fleet_id = $1::uuid AND a.settled_at IS NULL
    AND (a.producer <> $6 OR a.task_generation IS NULL OR EXISTS (
      SELECT 1 FROM fleet.tasks t WHERE t.id = a.task_id AND t.fleet_id = a.fleet_id
        AND t.workspace_id = a.workspace_id AND t.generation = a.task_generation
        AND t.terminal_at IS NULL))
  LIMIT $3
) AS outstanding) >= $2::bigint
AND NOT EXISTS (
  SELECT 1 FROM fleet.admissions WHERE producer = $4 AND producer_key = $5
) AS exceeded
