SELECT call.kind, call.tool, call.status, call.arguments::text AS arguments,
       call.truncated_arguments, call.output, call.output_line_count, call.truncated,
       call.changes::text AS changes, call.sources::text AS sources
FROM thread.items call
-- A run's items may name a turn its conversation folded in; the thread shows them under the
-- turn the run was assigned for, so the read finds them through that run. Once the run row
-- is deleted, `run_id` is NULL and the item stays with its turn: the run's head turn is
-- then the one its fence was stamped on, since a report settles only the turn whose
-- `run_fence` is its run's fence (`afd_fleet/src/run/sql/commit_report.sql`).
-- Two arms, so each is an index probe: the first reaches the call through the turn's runs
-- (`idx_runs_turn_id_fencing_token`, then `uq_items_run_id_sequence`), where one OR across
-- the join read every item of the fleet at that sequence.
WHERE call.workspace_id = $1 AND call.fleet_id = $2
  AND call.fencing_token = $4 AND call.sequence = $5
  AND call.run_id IN (SELECT r.id FROM thread.runs r WHERE r.turn_id = $3)
UNION ALL
SELECT call.kind, call.tool, call.status, call.arguments::text AS arguments,
       call.truncated_arguments, call.output, call.output_line_count, call.truncated,
       call.changes::text AS changes, call.sources::text AS sources
FROM thread.items call
WHERE call.workspace_id = $1 AND call.fleet_id = $2
  AND call.fencing_token = $4 AND call.sequence = $5 AND call.run_id IS NULL
  AND EXISTS (SELECT 1 FROM thread.turns h
               WHERE h.id = $3 AND h.thread_id = $2 AND h.run_fence = $4)
