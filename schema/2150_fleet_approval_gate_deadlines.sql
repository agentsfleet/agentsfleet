-- Parked heads probe standing access and unexpired event approvals separately.
CREATE INDEX idx_approval_gates_pending_scope_deadline
    ON fleet.approval_gates (fleet_id, event_id, expires_at)
    WHERE resolved_at IS NULL;
