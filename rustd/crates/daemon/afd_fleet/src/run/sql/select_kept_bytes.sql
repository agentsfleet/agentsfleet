SELECT COALESCE(SUM(t.byte_count), 0)::bigint
FROM thread.items t
WHERE t.fleet_id = $1 AND t.turn_id = $2 AND t.fencing_token = $3
