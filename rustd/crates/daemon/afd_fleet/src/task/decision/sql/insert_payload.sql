INSERT INTO runner.decision_payloads (
    decision_call_id, fleet_id, workspace_id, tenant_id, task_id, request, created_at, updated_at
) VALUES ($1, $2, $3, $4, $5, $6, clock_timestamp(), clock_timestamp())
