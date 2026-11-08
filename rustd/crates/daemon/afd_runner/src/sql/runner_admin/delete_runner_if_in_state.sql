WITH current_row AS (
  SELECT r.id, r.status,
         EXISTS (
           SELECT 1 FROM runner.leases l
           WHERE l.runner_id = r.id AND l.status = $3::text
         ) AS leased
  FROM runner.runners r
  WHERE r.id = $1::uuid
), deleted AS (
  DELETE FROM runner.runners r
  USING current_row c
  WHERE r.id = c.id AND c.status = $2::text AND NOT c.leased
  RETURNING r.id
)
SELECT TRUE AS changed, FALSE AS leased FROM deleted
UNION ALL
SELECT FALSE AS changed, c.leased FROM current_row c
WHERE NOT EXISTS (SELECT 1 FROM deleted)
LIMIT 1
