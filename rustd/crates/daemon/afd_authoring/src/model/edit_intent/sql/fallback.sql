INSERT INTO authoring.edit_decisions
    (dispatch_id, attempt_id, tenant_id, workspace_id, usage_sequence,
     candidate_revision, disposition, reserved_nanos, accounted_at, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, 0,
        clock_timestamp(), clock_timestamp(), clock_timestamp())
ON CONFLICT DO NOTHING
