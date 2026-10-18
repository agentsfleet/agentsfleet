SELECT
    EXISTS(SELECT 1 FROM fleet.fleets WHERE workspace_id = $1::uuid) AS has_fleet,
    EXISTS(SELECT 1 FROM vault.secrets WHERE workspace_id = $1::uuid) AS has_secret,
    EXISTS(SELECT 1 FROM fleet.events WHERE workspace_id = $1::uuid) AS has_event,
    EXISTS(SELECT 1 FROM fleet.events WHERE workspace_id = $1::uuid AND actor LIKE $2) AS has_steer,
    EXISTS(SELECT 1 FROM model.tenant_selections
           WHERE tenant_id = $3::uuid AND tenant_model_id IS NOT NULL) AS tenant_model
