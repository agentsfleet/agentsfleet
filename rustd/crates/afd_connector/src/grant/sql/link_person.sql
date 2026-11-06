INSERT INTO connector.person_links (workspace_id, install_id, team_id, external_user_id, user_id, created_at, updated_at)
SELECT w.id, i.id, $2, $3, u.id, clock_timestamp(), clock_timestamp()
FROM identity.workspaces w JOIN identity.users u ON u.oidc_subject = $4
JOIN connector.installs i ON i.workspace_id = w.id AND i.provider = 'slack' AND i.external_account_id = $2
WHERE w.id = $1::uuid
  AND (u.tenant_id = w.tenant_id OR EXISTS (
      SELECT 1 FROM identity.memberships m WHERE m.tenant_id = w.tenant_id AND m.user_id = u.id
  ))
ON CONFLICT (workspace_id, team_id, external_user_id)
DO UPDATE SET install_id = EXCLUDED.install_id, updated_at = EXCLUDED.updated_at
WHERE connector.person_links.user_id = EXCLUDED.user_id
