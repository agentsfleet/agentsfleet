SELECT id AS task_id, generation, stop_generation, accepted_revision_id,
       goal_digest, status AS state, covered_input_sequence, effect_cursor,
       confirmed_effect_watermark
FROM fleet.tasks
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
  AND expires_at > clock_timestamp()
  AND fleet.task_person_allowed(id, $4, $5, $6, $7)
