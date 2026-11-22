UPDATE fleet.durable_task_effect_states SET status = $2, updated_at = now()
WHERE durable_task_effect_id = $1 AND status = $3
  AND EXISTS (SELECT 1 FROM fleet.durable_task_effects e WHERE e.id = durable_task_effect_id AND e.issuance_expires_at > clock_timestamp())
