-- The gate's fleet and then its thread, before the gate: the order Stop and a decision's
-- issue statement take them. Stop locks the turn and then closes its gate, and this statement writes
-- the gate and then the fleet's turns, so without the thread the two could lock one gate
-- and one turn in opposite orders. The fleet comes first because fleet delete holds it
-- FOR UPDATE and then cascades to the gates and the thread: this waits there holding
-- nothing. An action id is not unique, so both read the set of fleets its open gates name,
-- within the caller's scope.
WITH fleet AS MATERIALIZED (
  SELECT f.id
  FROM fleet.fleets f
  WHERE f.id IN (SELECT g.fleet_id FROM fleet.approval_gates g
                  WHERE g.action_id = $4 AND g.resolved_at IS NULL
                    AND (g.fleet_id = $5 OR $5 IS NULL))
  ORDER BY f.id
  FOR KEY SHARE OF f
), thread AS MATERIALIZED (
  -- In id order, as the renewal batch takes threads, when an unscoped action names several.
  SELECT s.id
  FROM thread.threads s
  WHERE s.id IN (SELECT id FROM fleet)
  ORDER BY s.id
  FOR NO KEY UPDATE OF s
), resolved AS (
  UPDATE fleet.approval_gates
  SET status = $1, detail = $2, resolved_by = $3,
      resolved_at = now(), updated_at = now(), active_grant_id = NULL,
      resolved_by_name = (
        SELECT COALESCE(NULLIF(u.display_name, ''), u.email) FROM identity.users u
          JOIN fleet.fleets z ON z.id = fleet.approval_gates.fleet_id
          WHERE u.oidc_subject = $3 AND u.tenant_id = z.tenant_id)
  WHERE action_id = $4 AND resolved_at IS NULL
    AND (fleet_id = $5 OR $5 IS NULL)
    AND fleet_id IN (SELECT id FROM thread)
    AND NOT EXISTS (SELECT 1 FROM fleet.fleets z
                     WHERE z.id = fleet.approval_gates.fleet_id AND z.status = $10)
  RETURNING id, action_id, workspace_id, fleet_id, status, resolved_at,
            resolved_by, detail, gate_type, evidence, turn_id
), granted AS (
  -- A card's answer is what its grant becomes: Approve moves the asked scope into force,
  -- Deny revokes, and either clears the ask.
  UPDATE connector.integration_grants g
  SET status          = CASE WHEN r.status = $6 THEN $7 ELSE $8 END,
      scope           = CASE WHEN r.status = $6 THEN COALESCE(g.requested_scope, g.scope)
                             ELSE g.scope END,
      requested_scope = NULL,
      approved_at     = CASE WHEN r.status = $6 THEN now() ELSE g.approved_at END,
      revoked_at      = CASE WHEN r.status = $6 THEN NULL ELSE now() END,
      updated_at      = now()
  FROM resolved r
  WHERE g.fleet_id = r.fleet_id
    AND g.service  = r.evidence->>'service'
    AND g.grant_type = r.evidence->>'kind'
    AND r.gate_type = $9
  RETURNING g.id
)

SELECT id AS gate_id, action_id, workspace_id,
       fleet_id, status, resolved_at, resolved_by, detail, turn_id, gate_type,
       CASE WHEN evidence->>'kind' = $11 THEN evidence->>'service' END AS approved_host,
       CASE WHEN evidence->>'kind' = $11 THEN evidence->'redirect'->>'tool' END AS refused_tool,
       (SELECT COUNT(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = resolved.fleet_id AND g.resolved_at IS NULL
           AND g.id NOT IN (SELECT id FROM resolved)) AS pending_approvals
FROM resolved
