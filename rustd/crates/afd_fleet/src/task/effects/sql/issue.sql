UPDATE fleet.task_effect_states SET status = $2, issued_at = clock_timestamp(), updated_at = now()
WHERE task_effect_id = $1::uuid AND status = $3
  AND EXISTS (SELECT 1 FROM fleet.task_effects e WHERE e.id = task_effect_id AND e.issuance_expires_at > clock_timestamp())
