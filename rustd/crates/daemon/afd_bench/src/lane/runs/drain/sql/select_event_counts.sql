SELECT count(*), count(*) FILTER (WHERE status = $2)
FROM thread.turns
WHERE thread_id = ANY($1) AND opened_at IS NOT NULL
