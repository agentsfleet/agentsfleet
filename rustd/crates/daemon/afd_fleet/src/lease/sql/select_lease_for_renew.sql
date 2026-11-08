SELECT tenant_id, fleet_id,
       workspace_id, posture, provider, model, status
FROM runner.leases WHERE id = $1 AND runner_id = $2
