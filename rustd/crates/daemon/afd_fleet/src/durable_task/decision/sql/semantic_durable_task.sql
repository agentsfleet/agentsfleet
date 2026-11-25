SELECT id AS durable_task_id, generation, stop_generation,
       goal_digest, status AS state, covered_input_sequence, effect_cursor,
       confirmed_effect_watermark
FROM fleet.durable_tasks
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
  AND expires_at > clock_timestamp()
  AND fleet.durable_task_person_allowed(id, $4, $5, $6, $7)
