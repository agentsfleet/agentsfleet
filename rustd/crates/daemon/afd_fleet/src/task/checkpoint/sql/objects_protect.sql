UPDATE fleet.task_objects
SET reclaim_after_at = GREATEST(reclaim_after_at, $3::timestamptz + make_interval(secs => $2)), updated_at = now()
WHERE id = ANY($1)
