DELETE FROM fleet.tool_calls t
USING fleet.events e
WHERE e.id = t.fleet_event_id AND e.fleet_id = $1::uuid AND e.event_id = $2
  AND t.fencing_token <> $3
