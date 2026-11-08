INSERT INTO fleet.tool_calls AS t
  (id, workspace_id, fleet_id, fleet_event_id, fencing_token, sequence, arguments,
   truncated_arguments, output, output_line_count, truncated, byte_count,
   created_at, updated_at)
SELECT r.id, $1::uuid, $2::uuid, e.id, $4, r.sequence, r.arguments::jsonb,
       r.truncated_arguments, r.output, r.output_line_count, r.truncated,
       r.byte_count, now(), now()
FROM UNNEST($5::uuid[], $6::bigint[], $7::text[], $8::bool[], $9::text[],
            $10::bigint[], $11::bool[], $12::bigint[])
  AS r(id, sequence, arguments, truncated_arguments, output,
       output_line_count, truncated, byte_count)
JOIN fleet.events e ON e.fleet_id = $2::uuid AND e.event_id = $3
ON CONFLICT (fleet_event_id, fencing_token, sequence) DO UPDATE SET
  arguments = EXCLUDED.arguments,
  truncated_arguments = EXCLUDED.truncated_arguments,
  output = EXCLUDED.output,
  output_line_count = EXCLUDED.output_line_count,
  truncated = EXCLUDED.truncated,
  byte_count = EXCLUDED.byte_count,
  updated_at = now()
