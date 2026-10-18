SELECT tenant_id::text AS tenant_id, fleet_id::text AS fleet_id,
       workspace_id::text AS workspace_id, posture, provider, model, status
FROM runner.leases WHERE id = $1::uuid AND runner_id = $2::uuid
