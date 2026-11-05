WITH slot AS MATERIALIZED (
  SELECT s.fleet_id FROM runner.fleet_slots s
  WHERE s.fleet_id = $1::uuid AND s.fencing_token = $3
  FOR UPDATE OF s
), settled AS (
  UPDATE fleet.admissions a
  SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
  FROM slot s
  WHERE a.fleet_id = s.fleet_id AND a.id = $2::uuid AND a.lease_fence = $3
    AND a.settled_at IS NULL
  RETURNING a.id
)
UPDATE runner.fleet_slots s
SET lease_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
    last_runner_id = CASE WHEN s.hold_expires_at > clock_timestamp() THEN s.last_runner_id END
FROM slot l
WHERE s.fleet_id = l.fleet_id AND EXISTS (SELECT 1 FROM settled)
