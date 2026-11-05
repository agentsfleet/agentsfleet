UPDATE fleet.task_objects
SET reclaim_after = GREATEST(reclaim_after, $3::timestamptz + make_interval(secs => $2)), updated_at = now()
WHERE id = ANY($1::text[]::uuid[])
