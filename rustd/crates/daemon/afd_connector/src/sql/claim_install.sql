INSERT INTO connector.installs (id, provider, external_account_id, workspace_id, scopes, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5::text[], now(), now())
ON CONFLICT (provider, external_account_id) DO UPDATE SET
    scopes = EXCLUDED.scopes,
    updated_at = now()
WHERE connector.installs.workspace_id = EXCLUDED.workspace_id
