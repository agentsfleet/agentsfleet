SELECT g.id AS gate_id, g.action_id, g.workspace_id,
       g.fleet_id, g.status,
       COALESCE(g.resolved_at, g.updated_at) AS resolved_at, COALESCE(g.resolved_by, '') AS resolved_by,
       g.detail, g.turn_id, g.gate_type,
       CASE WHEN g.evidence->>'kind' = $3 THEN g.evidence->>'service' END AS approved_host,
       CASE WHEN g.evidence->>'kind' = $3 THEN g.evidence->'redirect'->>'tool' END AS refused_tool,
       g.resolved_at IS NULL AS unanswered
FROM fleet.approval_gates g
WHERE g.action_id = $1
  AND (g.fleet_id = $2 OR $2 IS NULL)
ORDER BY g.created_at DESC, g.id DESC LIMIT 1
