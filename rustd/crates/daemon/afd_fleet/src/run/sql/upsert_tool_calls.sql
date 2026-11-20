INSERT INTO thread.items AS t
  (id, workspace_id, fleet_id, run_id, turn_id, fencing_token, sequence, kind, tool, status,
   arguments, truncated_arguments, output, output_line_count, truncated, byte_count, changes,
   created_at, updated_at)
SELECT r.id, $1, $2, $13, e.id, $4, r.sequence, r.kind, r.tool, r.status,
       r.arguments::jsonb, r.truncated_arguments, r.output, r.output_line_count, r.truncated,
       r.byte_count, r.changes::jsonb, now(), now()
FROM UNNEST($5::uuid[], $6::bigint[], $7::text[], $8::bool[], $9::text[],
            $10::bigint[], $11::bool[], $12::bigint[], $14::uuid[], $15::text[],
            $16::text[], $17::text[], $18::text[])
  AS r(id, sequence, arguments, truncated_arguments, output,
       output_line_count, truncated, byte_count, turn_id, kind, tool, status, changes)
-- A turn the run serves: its own, opened, or one its conversation folded in at this fence.
JOIN thread.turns e
  ON e.thread_id = $2 AND e.id = r.turn_id
 AND ((e.id = $3 AND e.opened_at IS NOT NULL)
      OR (e.input_run_id = $13 AND e.input_fence = $4))
ON CONFLICT (run_id, sequence) DO UPDATE SET
  turn_id = EXCLUDED.turn_id,
  kind = EXCLUDED.kind,
  tool = EXCLUDED.tool,
  status = EXCLUDED.status,
  arguments = EXCLUDED.arguments,
  truncated_arguments = EXCLUDED.truncated_arguments,
  output = EXCLUDED.output,
  output_line_count = EXCLUDED.output_line_count,
  truncated = EXCLUDED.truncated,
  byte_count = EXCLUDED.byte_count,
  changes = EXCLUDED.changes,
  updated_at = now()
