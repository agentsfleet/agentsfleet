WITH current_row AS (
  SELECT r.id, r.status,
         -- A live run, or an ended one whose runtime tail is not yet billed: the
         -- delete cascades to runs, so either would lose work or money.
         EXISTS (
           SELECT 1 FROM runner.runs l
           WHERE l.runner_id = r.id
             AND (l.status = $3::text OR l.last_metered_at < l.run_expires_at)
         ) AS held
  FROM runner.runners r
  WHERE r.id = $1 AND ($5::boolean OR r.tenant_id = $4::uuid)
), deleted AS (
  DELETE FROM runner.runners r
  USING current_row c
  WHERE r.id = c.id AND c.status = $2::text AND NOT c.held
  RETURNING r.id
)
SELECT TRUE AS changed, FALSE AS held FROM deleted
UNION ALL
SELECT FALSE AS changed, c.held FROM current_row c
WHERE NOT EXISTS (SELECT 1 FROM deleted)
LIMIT 1
