SELECT d.fleet_id AS fleet_id, u.oidc_subject
  FROM authoring.drafts d
 JOIN authoring.messages m ON m.draft_id = d.id
 JOIN identity.users u ON u.id = d.user_id
 WHERE d.id = $1 AND d.workspace_id = $2 AND d.user_id = $3
   AND m.sequence = $4 AND m.save_requested AND m.status = $5
   AND d.status = $6 AND d.fleet_id IS NOT NULL
   AND d.tenant_id = $7
   AND (u.tenant_id = d.tenant_id OR EXISTS (
       SELECT 1 FROM identity.memberships membership
        WHERE membership.tenant_id = d.tenant_id AND membership.user_id = u.id
   ))
