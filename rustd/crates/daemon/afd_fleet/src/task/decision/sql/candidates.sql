SELECT id AS task_id, generation, stop_generation, accepted_revision_id,
       goal_digest, status AS state, covered_input_sequence, effect_cursor,
       confirmed_effect_watermark, title, target
FROM fleet.tasks
WHERE fleet_id = $1 AND workspace_id = $2 AND terminal_at IS NULL
ORDER BY created_at, id
LIMIT $3
