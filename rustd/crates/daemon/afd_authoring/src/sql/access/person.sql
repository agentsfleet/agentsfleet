SELECT u.id AS user_id,
       w.tenant_id AS owner_id,
       u.tenant_id = w.tenant_id AS personal_owner
  FROM identity.users u
  JOIN identity.workspaces w ON w.id = $1
 WHERE u.oidc_subject = $2
   FOR SHARE OF u
