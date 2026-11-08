INSERT INTO connector.installs (id, provider, external_account_id, workspace_id, scopes, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4::uuid, $5::text[], now(), now())
ON CONFLICT (provider, external_account_id) DO UPDATE SET
    workspace_id = EXCLUDED.workspace_id,
    scopes = EXCLUDED.scopes,
    updated_at = now()
