WITH expired AS (
  UPDATE runner.fleet_slots s
  SET lease_expires_at = statement_timestamp() - $3::interval, updated_at = now()
  WHERE s.last_runner_id = $1::uuid
    AND s.lease_expires_at > statement_timestamp() - $3::interval
    AND s.fleet_id IN (
      SELECT l.fleet_id FROM runner.leases l
      WHERE l.runner_id = $1::uuid AND l.status = $2::text
    )
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM expired
