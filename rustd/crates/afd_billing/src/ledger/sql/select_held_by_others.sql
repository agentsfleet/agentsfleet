SELECT COALESCE(SUM(GREATEST(ceiling_nanos - captured_nanos, 0)), 0)::bigint AS held
FROM billing.holds
WHERE tenant_id = $1::uuid AND released_at IS NULL
  AND NOT (work_type = $2 AND work_id = $3::uuid)
