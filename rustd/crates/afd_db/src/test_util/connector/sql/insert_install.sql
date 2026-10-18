INSERT INTO connector.installs
  (id, provider, external_account_id, workspace_id, scopes, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4::uuid, $5, coalesce($6::timestamptz, now()),
        coalesce($6::timestamptz, now()))
