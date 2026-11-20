SELECT t.sequence, t.byte_count
FROM thread.items t
WHERE t.run_id = $1
  AND t.sequence = ANY($2::bigint[])
