-- One actionable card per grant. Answered and expired cards release the
-- reference, so history remains without encoding application status vocabulary.
CREATE UNIQUE INDEX IF NOT EXISTS uq_fleet_approval_gates_active_grant_id
    ON core.fleet_approval_gates (active_grant_id);
