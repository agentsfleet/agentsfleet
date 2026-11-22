SELECT
    EXISTS(SELECT 1 FROM fleet.fleets WHERE workspace_id = $1) AS has_fleet,
    EXISTS(SELECT 1 FROM vault.secrets WHERE workspace_id = $1) AS has_secret,
    EXISTS(SELECT 1 FROM thread.turns WHERE workspace_id = $1 AND opened_at IS NOT NULL) AS has_event,
    EXISTS(SELECT 1 FROM thread.turns WHERE workspace_id = $1 AND opened_at IS NOT NULL
           AND actor LIKE $2) AS has_steer,
    EXISTS(SELECT 1 FROM model.tenant_models
           WHERE tenant_id = $3 AND selected) AS tenant_model
