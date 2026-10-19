SELECT call.arguments::text AS arguments, call.truncated_arguments, call.output,
       call.output_line_count, call.truncated
FROM fleet.tool_calls call
JOIN fleet.events event ON event.id = call.fleet_event_id
WHERE call.workspace_id = $1::uuid AND call.fleet_id = $2::uuid
  AND event.fleet_id = $2::uuid AND event.event_id = $3
  AND call.fencing_token = $4 AND call.sequence = $5
