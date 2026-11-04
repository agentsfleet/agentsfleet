SELECT id::text AS task_id, generation, stop_generation, accepted_revision_id::text,
       goal_digest, status AS state, covered_input_sequence, effect_cursor,
       confirmed_effect_watermark
FROM fleet.tasks
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid
  AND expires_at > clock_timestamp()
