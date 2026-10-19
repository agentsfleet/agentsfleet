SELECT COALESCE(SUM(t.byte_count), 0)::bigint
FROM fleet.tool_calls t
JOIN fleet.events e ON e.id = t.fleet_event_id
WHERE e.fleet_id = $1::uuid AND e.event_id = $2 AND t.fencing_token = $3
