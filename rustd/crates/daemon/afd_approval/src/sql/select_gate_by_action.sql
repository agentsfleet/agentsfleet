SELECT id::text AS gate_id, action_id, workspace_id::text AS workspace_id,
       fleet_id::text AS fleet_id, status,
       COALESCE(resolved_at, updated_at) AS resolved_at, resolved_by, detail, event_id
FROM fleet.approval_gates
WHERE action_id = $1
  AND ($2::text = '' OR fleet_id::text = $2)
ORDER BY created_at DESC, id DESC LIMIT 1
