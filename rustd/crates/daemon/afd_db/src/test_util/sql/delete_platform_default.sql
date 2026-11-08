DELETE FROM model.platform_defaults WHERE provider = $1 AND source_workspace_id = $2::uuid
