SELECT g.id AS gate_id, g.fleet_id,
       COALESCE(z.name, '') AS fleet_name, g.workspace_id,
       g.action_id, g.tool_name, g.action_name, g.gate_type, g.proposed_action,
       g.evidence::text AS evidence_json, g.blast_radius, d.status, g.detail,
       g.created_at, g.expires_at, g.resolved_at, g.resolved_by, g.resolved_by_name
FROM fleet.approval_gates g
JOIN fleet.fleets z ON z.id = g.fleet_id
-- Nothing expires a gate: an unanswered one past its deadline reads `timed_out` and
-- stays answerable.
CROSS JOIN LATERAL (SELECT CASE WHEN g.resolved_at IS NULL AND g.expires_at <= now()
                                THEN $3 ELSE g.status END AS status) d
WHERE g.id = $1 AND g.workspace_id = $2
