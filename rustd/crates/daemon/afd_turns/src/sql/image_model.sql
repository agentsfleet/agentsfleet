SELECT tenant_id, model_ref
FROM fleet.fleets
WHERE id = $1 AND workspace_id = $2
