SELECT status, expires_at <= statement_timestamp() AS lapsed
FROM fleet.approval_gates
WHERE action_id = $1
ORDER BY created_at DESC, id DESC LIMIT 1
