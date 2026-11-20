SELECT call.arguments::text AS arguments, call.truncated_arguments, call.output,
       call.output_line_count, call.truncated, call.changes::text AS changes
FROM thread.items call
-- A run's items may name a turn its conversation folded in; the thread shows them under the
-- turn the run was assigned for, so the read finds them through that run.
JOIN thread.runs r ON r.id = call.run_id
WHERE call.workspace_id = $1 AND call.fleet_id = $2 AND r.turn_id = $3
  AND call.fencing_token = $4 AND call.sequence = $5
