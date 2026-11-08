SELECT t.sequence, t.byte_count
FROM fleet.tool_calls t
WHERE t.fleet_id = $1 AND t.event_id = $2 AND t.fencing_token = $3
  AND t.sequence = ANY($4::bigint[])
