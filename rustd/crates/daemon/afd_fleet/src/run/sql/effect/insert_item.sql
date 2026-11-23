-- One call's item, written with its effect's answer before the daemon responds. The run's
-- record post later fills in the call's own arguments and output on the same
-- `(run_id, sequence)` row and keeps its `id`.
--
-- $1 id · $2 workspace · $3 fleet · $4 run · $5 turn · $6 fence · $7 the call's number
-- · $8 kind · $9 tool · $10 status · $11 arguments as JSON text · $12 output
-- · $13 output line count
INSERT INTO thread.items
  (id, workspace_id, fleet_id, run_id, turn_id, fencing_token, sequence, kind, tool, status,
   arguments, truncated_arguments, output, output_line_count, truncated, changes,
   created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, false, $12, $13, false, NULL,
        now(), now())
ON CONFLICT (run_id, sequence) DO NOTHING
