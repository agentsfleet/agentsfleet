SELECT COALESCE(SUM(t.byte_count), 0)::bigint
FROM thread.items t
WHERE t.run_id = $1
