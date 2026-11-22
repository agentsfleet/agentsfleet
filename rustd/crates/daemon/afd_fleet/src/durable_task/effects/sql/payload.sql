INSERT INTO fleet.durable_task_effect_payloads (durable_task_effect_id, durable_task_id, fleet_id, workspace_id, target, request, created_at)
VALUES ($1, $2, $3, $4, $5, $6::jsonb, now())
