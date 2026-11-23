WITH held AS MATERIALIZED (
    SELECT id FROM identity.lock_user($1, true)
), person AS MATERIALIZED (
    SELECT u.id, u.tenant_id, w.id AS workspace_id, w.tenant_id AS owner
      FROM held h
      JOIN identity.users u ON u.id = h.id
      JOIN identity.workspaces w ON w.id = $2
), membership AS MATERIALIZED (
    SELECT m.id, m.role, m.created_at
      FROM identity.memberships m
      JOIN person p ON m.tenant_id = p.owner AND m.user_id = p.id
)
SELECT encode(sha256(convert_to(jsonb_build_array(
           p.id, p.tenant_id, p.workspace_id, p.owner, m.id, m.role, m.created_at
       )::text, 'UTF8')), 'hex')
  FROM person p
  LEFT JOIN membership m ON true
 WHERE p.tenant_id = p.owner OR m.id IS NOT NULL
