WITH person AS MATERIALIZED (
    SELECT l.id
      FROM identity.users u
     CROSS JOIN LATERAL identity.lock_user(u.id, true) l
     WHERE u.oidc_subject = $2
)
SELECT u.id AS user_id,
       w.tenant_id AS owner_id,
       u.tenant_id = w.tenant_id AS personal_owner
  FROM person p
  JOIN identity.users u ON u.id = p.id
  JOIN identity.workspaces w ON w.id = $1
