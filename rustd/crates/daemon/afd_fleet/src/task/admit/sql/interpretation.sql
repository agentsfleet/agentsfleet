INSERT INTO fleet.task_interpretations (turn_id, fleet_id, workspace_id, tenant_id, plan_digest,
    message, plan, source_actor, source_producer, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
