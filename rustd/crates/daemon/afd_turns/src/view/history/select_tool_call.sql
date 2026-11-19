SELECT call.arguments::text AS arguments, call.truncated_arguments, call.output,
       call.output_line_count, call.truncated
FROM thread.items call
WHERE call.workspace_id = $1 AND call.fleet_id = $2 AND call.turn_id = $3
  AND call.fencing_token = $4 AND call.sequence = $5
