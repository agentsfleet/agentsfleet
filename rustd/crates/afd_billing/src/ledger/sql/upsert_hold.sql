INSERT INTO billing.holds
    (id, work_type, work_id, tenant_id, ceiling_nanos, captured_nanos, created_at, updated_at)
VALUES ($1::uuid, $2, $3::uuid, $4::uuid, $5, 0, now(), now())
ON CONFLICT (work_type, work_id) DO UPDATE
    SET ceiling_nanos = GREATEST(billing.holds.ceiling_nanos, EXCLUDED.ceiling_nanos),
        updated_at = now()
    WHERE billing.holds.released_at IS NULL
