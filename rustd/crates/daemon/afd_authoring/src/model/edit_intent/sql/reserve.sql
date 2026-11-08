INSERT INTO authoring.edit_decisions
    (dispatch_id, attempt_id, tenant_id, workspace_id, usage_sequence,
     candidate_revision, snapshot, disposition, reserved_nanos, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, $9, $9)
ON CONFLICT DO NOTHING
