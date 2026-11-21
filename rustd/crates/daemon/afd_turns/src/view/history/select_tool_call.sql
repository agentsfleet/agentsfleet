SELECT call.kind, call.tool, call.status, call.arguments::text AS arguments,
       call.truncated_arguments, call.output, call.output_line_count, call.truncated,
       call.changes::text AS changes
FROM thread.items call
-- A run's items may name a turn its conversation folded in; the thread shows them under the
-- turn the run was assigned for, so the read finds them through that run. Once the run row
-- is deleted, `run_id` is NULL and the item stays with its turn: the run's head turn is
-- then the one its fence was stamped on, since a report settles only the turn whose
-- `run_fence` is its run's fence (`afd_fleet/src/run/sql/commit_report.sql`).
LEFT JOIN thread.runs r ON r.id = call.run_id
WHERE call.workspace_id = $1 AND call.fleet_id = $2
  AND call.fencing_token = $4 AND call.sequence = $5
  AND (r.turn_id = $3
       OR (call.run_id IS NULL
           AND EXISTS (SELECT 1 FROM thread.turns h
                        WHERE h.id = $3 AND h.thread_id = $2 AND h.run_fence = $4)))
