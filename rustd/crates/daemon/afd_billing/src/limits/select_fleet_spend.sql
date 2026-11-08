-- Floors are cut to the millisecond the TIMESTAMPTZ(3) columns hold, so a line
-- stamped exactly at the day floor counts.
WITH floors AS (
  SELECT date_trunc('milliseconds', now() - $3::interval) AS day_floor,
         date_trunc('month', now(), 'UTC') AS month_floor
)
SELECT
  COALESCE(SUM(l.charged_nanos) FILTER (WHERE l.created_at >= f.day_floor), 0)::bigint AS day_nanos,
  COALESCE(SUM(l.charged_nanos) FILTER (WHERE l.created_at >= f.month_floor), 0)::bigint AS month_nanos
FROM floors f
JOIN billing.usage_lines l
  ON l.workspace_id = $1::uuid AND l.fleet_id = $2::uuid
 AND l.created_at >= LEAST(f.day_floor, f.month_floor)
