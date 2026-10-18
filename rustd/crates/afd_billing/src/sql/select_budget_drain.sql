-- Floors are cut to the millisecond the TIMESTAMPTZ(3) charge columns hold.
-- now() carries microseconds, so an untruncated day floor would sit a few
-- microseconds past a charge stamped "at" it, and the inclusive floor test
-- would drop that charge. The month floor is already whole.
WITH floors AS (
  SELECT date_trunc('milliseconds', now() - $3::interval) AS day_floor,
         date_trunc('month', now(), 'UTC') AS month_floor
)
SELECT
  COALESCE(SUM(CASE
    WHEN c.updated_at < f.day_floor THEN 0
    WHEN c.created_at >= f.day_floor THEN c.charged_nanos
    ELSE c.charged_nanos::numeric * EXTRACT(EPOCH FROM c.updated_at - f.day_floor)
         / NULLIF(EXTRACT(EPOCH FROM c.updated_at - c.created_at), 0)
  END), 0)::bigint AS day_nanos,
  COALESCE(SUM(CASE
    WHEN c.updated_at < f.month_floor THEN 0
    WHEN c.created_at >= f.month_floor THEN c.charged_nanos
    ELSE c.charged_nanos::numeric * EXTRACT(EPOCH FROM c.updated_at - f.month_floor)
         / NULLIF(EXTRACT(EPOCH FROM c.updated_at - c.created_at), 0)
  END), 0)::bigint AS month_nanos
FROM floors f
JOIN billing.fleet_charges c
  ON c.workspace_id = $1::uuid AND c.fleet_id = $2::uuid
 AND c.charge_type IN ($4, $5)
 AND c.updated_at >= LEAST(f.day_floor, f.month_floor)
