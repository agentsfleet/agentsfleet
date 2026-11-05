INSERT INTO fleet.task_interpretations (admission_id, fleet_id, workspace_id, tenant_id, plan_digest,
    message, plan, accepting_lease_id, accepting_fence, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7, $8::uuid, $9, now())
