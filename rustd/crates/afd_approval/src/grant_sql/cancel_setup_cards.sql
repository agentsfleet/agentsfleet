UPDATE fleet.approval_gates
SET status = $2, resolved_by = $3, detail = $4,
    resolved_at = now(), updated_at = now(), active_grant_id = NULL
WHERE fleet_id = $1::uuid AND resolved_at IS NULL AND gate_type = $5
