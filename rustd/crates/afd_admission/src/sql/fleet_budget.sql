SELECT (SELECT count(*) FROM (
  SELECT 1 FROM fleet.admissions
  WHERE fleet_id = $1::uuid AND settled_at IS NULL LIMIT $3
) AS outstanding) >= $2::bigint
AND NOT EXISTS (
  SELECT 1 FROM fleet.admissions WHERE producer = $4 AND producer_key = $5
) AS exceeded
