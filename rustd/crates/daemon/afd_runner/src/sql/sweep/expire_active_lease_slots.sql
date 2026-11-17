WITH expired AS (
  UPDATE runner.fleet_slots s
  SET lease_expires_at = statement_timestamp() - $3::interval, updated_at = now()
  -- From the runner's live leases to each slot by its key, so no slot index needs
  -- the deadline every renewal rewrites.
  FROM (SELECT DISTINCT l.fleet_id FROM runner.leases l
        WHERE l.runner_id = $1 AND l.status = $2::text) live
  WHERE s.fleet_id = live.fleet_id
    AND s.last_runner_id = $1
    AND s.lease_expires_at > statement_timestamp() - $3::interval
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM expired
