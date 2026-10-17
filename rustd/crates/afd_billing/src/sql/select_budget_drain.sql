SELECT
  COALESCE(SUM(CASE
    WHEN c.updated_at < $3::timestamptz THEN 0
    WHEN c.created_at >= $3::timestamptz THEN c.charged_nanos
    ELSE c.charged_nanos::numeric * EXTRACT(EPOCH FROM c.updated_at - $3::timestamptz)
         / NULLIF(EXTRACT(EPOCH FROM c.updated_at - c.created_at), 0)
  END), 0)::bigint AS day_nanos,
  COALESCE(SUM(CASE
    WHEN c.updated_at < $4::timestamptz THEN 0
    WHEN c.created_at >= $4::timestamptz THEN c.charged_nanos
    ELSE c.charged_nanos::numeric * EXTRACT(EPOCH FROM c.updated_at - $4::timestamptz)
         / NULLIF(EXTRACT(EPOCH FROM c.updated_at - c.created_at), 0)
  END), 0)::bigint AS month_nanos
FROM billing.fleet_charges c
WHERE c.workspace_id = $1::uuid AND c.fleet_id = $2::uuid
  AND c.charge_type IN ($5, $6)
  AND c.updated_at >= LEAST($3::timestamptz, $4::timestamptz)
