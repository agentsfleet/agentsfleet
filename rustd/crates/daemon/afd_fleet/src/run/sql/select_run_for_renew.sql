SELECT tenant_id, fleet_id,
       workspace_id, posture, provider, model, status,
       cancel_requested_at IS NOT NULL AS cancelled
FROM runner.runs WHERE id = $1 AND runner_id = $2 AND fleet_id IS NOT NULL
