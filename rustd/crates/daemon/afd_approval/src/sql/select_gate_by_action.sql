SELECT id AS gate_id, action_id, workspace_id,
       fleet_id, status,
       COALESCE(resolved_at, updated_at) AS resolved_at, resolved_by, detail, event_id
FROM fleet.approval_gates
WHERE action_id = $1
  AND (fleet_id = $2 OR $2 IS NULL)
ORDER BY created_at DESC, id DESC LIMIT 1
