SELECT id::text AS task_id, generation, stop_generation, accepted_revision_id::text,
       goal_digest, status AS state, covered_input_sequence, effect_cursor,
       confirmed_effect_watermark, title, target
FROM fleet.tasks
WHERE fleet_id = $1::uuid AND workspace_id = $2::uuid AND terminal_at IS NULL
ORDER BY created_at, id
LIMIT $3
