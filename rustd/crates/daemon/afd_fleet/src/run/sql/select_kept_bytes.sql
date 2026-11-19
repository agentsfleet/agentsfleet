SELECT COALESCE(SUM(t.byte_count), 0)::bigint
FROM fleet.tool_calls t
WHERE t.fleet_id = $1 AND t.event_id = $2 AND t.fencing_token = $3
