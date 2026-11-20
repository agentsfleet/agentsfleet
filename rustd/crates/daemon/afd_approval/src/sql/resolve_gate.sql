WITH resolved AS (
  UPDATE fleet.approval_gates
  SET status = $1, detail = $2, resolved_by = $3,
      resolved_at = now(), updated_at = now(), active_grant_id = NULL,
      resolved_by_name = (
        SELECT COALESCE(NULLIF(u.display_name, ''), u.email) FROM identity.users u
          JOIN fleet.fleets z ON z.id = fleet.approval_gates.fleet_id
          WHERE u.oidc_subject = $3 AND u.tenant_id = z.tenant_id)
  WHERE action_id = $4 AND resolved_at IS NULL
    AND (fleet_id = $5 OR $5 IS NULL)
  RETURNING id, action_id, workspace_id, fleet_id, status, resolved_at,
            resolved_by, detail, gate_type, evidence, turn_id
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
    AND r.turn_id IS NULL
    AND g.status != $8
  RETURNING g.id
), unparked AS (
  UPDATE thread.turns a SET parked_at = NULL, updated_at = now()
  FROM resolved r
  -- Parked rows and assigned ones: a park in flight holds an assigned row's lock and
  -- this waits on it, so the park cannot strand its flag. The unassigned backlog is
  -- neither rewritten nor locked.
  WHERE a.thread_id = r.fleet_id AND a.settled_at IS NULL
    AND (a.parked_at IS NOT NULL OR a.run_fence IS NOT NULL)
)

SELECT id AS gate_id, action_id, workspace_id,
       fleet_id, status, resolved_at, resolved_by, detail, turn_id,
       (SELECT COUNT(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = resolved.fleet_id AND g.resolved_at IS NULL
           AND g.id NOT IN (SELECT id FROM resolved)) AS pending_approvals
FROM resolved
