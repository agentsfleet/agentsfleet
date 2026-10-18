WITH resolved AS (
  UPDATE fleet.approval_gates
  SET status = $1, detail = $2, resolved_by = $3,
      resolved_at = now(), updated_at = now(), active_grant_id = NULL,
      resolved_by_name = (
        SELECT COALESCE(NULLIF(u.display_name, ''), u.email) FROM identity.users u
          JOIN fleet.fleets z ON z.id = fleet.approval_gates.fleet_id
          WHERE u.oidc_subject = $3 AND u.tenant_id = z.tenant_id)
  WHERE action_id = $4 AND resolved_at IS NULL
    AND ($5::text = '' OR fleet_id::text = $5)
  RETURNING id, action_id, workspace_id, fleet_id, status, resolved_at,
            resolved_by, detail, gate_type, evidence, event_id
), granted AS (
  UPDATE connector.integration_grants g
  SET status      = CASE WHEN r.status = $6 THEN $7 ELSE $8 END,
      approved_at = CASE WHEN r.status = $6 THEN now() END,
      revoked_at  = CASE WHEN r.status = $6 THEN NULL ELSE now() END,
      updated_at  = now()
  FROM resolved r
  WHERE g.fleet_id = r.fleet_id
    AND g.service  = r.evidence->>'service'
    AND r.gate_type = $9
    AND r.event_id IS NULL
    AND g.status != $8
  RETURNING g.id
)
SELECT id::text AS gate_id, action_id, workspace_id::text AS workspace_id,
       fleet_id::text AS fleet_id, status, resolved_at, resolved_by, detail, event_id,
       (SELECT COUNT(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = resolved.fleet_id AND g.resolved_at IS NULL
           AND g.id NOT IN (SELECT id FROM resolved)) AS pending_approvals
FROM resolved
