INSERT INTO fleet.task_effect_payloads (effect_id, task_id, fleet_id, workspace_id, target, request, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6::jsonb, now())
