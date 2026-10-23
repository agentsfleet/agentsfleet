SELECT action_id
FROM fleet.approval_gates
WHERE fleet_id = $1::uuid AND event_id = $2
ORDER BY created_at DESC, id DESC
LIMIT 1
