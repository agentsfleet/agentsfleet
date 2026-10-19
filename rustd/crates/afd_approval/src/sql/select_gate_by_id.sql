SELECT g.id::text AS gate_id, g.fleet_id::text AS fleet_id,
       COALESCE(z.name, '') AS fleet_name, g.workspace_id::text AS workspace_id,
       g.action_id, g.tool_name, g.action_name, g.gate_type, g.proposed_action,
       g.evidence::text AS evidence_json, g.blast_radius, g.status, g.detail,
       g.created_at, g.expires_at, g.resolved_at, g.resolved_by, g.resolved_by_name
FROM fleet.approval_gates g
JOIN fleet.fleets z ON z.id = g.fleet_id
WHERE g.id = $1::uuid AND g.workspace_id = $2::uuid
