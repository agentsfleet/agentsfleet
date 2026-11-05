UPDATE fleet.task_effect_states SET status = $2, issued_at = clock_timestamp(), updated_at = now()
WHERE effect_id = $1::uuid AND status = $3
  AND EXISTS (SELECT 1 FROM fleet.task_effects e WHERE e.id = effect_id AND e.issuance_deadline > clock_timestamp())
